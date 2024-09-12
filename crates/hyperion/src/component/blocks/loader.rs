use std::{borrow::Cow, cell::RefCell, collections::HashSet, io::Write, sync::Arc};

use anyhow::{bail, Context};
use bytes::BytesMut;
use fxhash::FxHashSet;
use glam::I16Vec2;
use itertools::Itertools;
use libdeflater::{CompressionLvl, Compressor};
use tracing::warn;
use valence_anvil::parsing::parse_chunk;
use valence_generated::block::BlockState;
use valence_nbt::{compound, List};
use valence_protocol::{packets::play, ChunkPos, CompressionThreshold, FixedArray};
use valence_registry::RegistryIdx;
use valence_server::layer::chunk::{
    bit_width, BiomeContainer, BlockStateContainer, Chunk, UnloadedChunk,
};

use crate::{
    bits::BitStorage,
    chunk::heightmap,
    component::blocks::{chunk::LoadedChunk, shared::Shared},
    net::encoder::PacketEncoder,
    runtime::AsyncRuntime,
    Scratch,
};

pub const CHUNK_HEIGHT_SPAN: u32 = 384;

struct TasksState {
    bytes: BytesMut,
    compressor: Compressor,
    scratch: Scratch,
}

impl Default for TasksState {
    fn default() -> Self {
        Self {
            bytes: BytesMut::new(),
            compressor: Compressor::new(CompressionLvl::new(6).unwrap()),
            scratch: Scratch::default(),
        }
    }
}

thread_local! {
  static STATE: RefCell<TasksState> = RefCell::new(TasksState::default());
}

struct Message {
    position: I16Vec2,
    tx: tokio::sync::mpsc::UnboundedSender<LoadedChunk>,
}

struct LaunchManager {
    rx_load_chunk_requests: tokio::sync::mpsc::UnboundedReceiver<Message>,
    received_request: FxHashSet<I16Vec2>,
    shared: Arc<Shared>,
    runtime: AsyncRuntime,
}

pub struct LaunchHandle {
    tx_load_chunk_requests: tokio::sync::mpsc::UnboundedSender<Message>,
}

impl LaunchHandle {
    pub fn send(&self, position: I16Vec2, tx: tokio::sync::mpsc::UnboundedSender<LoadedChunk>) {
        self.tx_load_chunk_requests
            .send(Message { position, tx })
            .unwrap();
    }
}

pub fn launch_manager(shared: Arc<Shared>, runtime: AsyncRuntime) -> LaunchHandle {
    let (tx_load_chunk_requests, rx_load_chunk_requests) = tokio::sync::mpsc::unbounded_channel();

    runtime.spawn({
        let runtime = runtime.clone();
        async move {
            LaunchManager {
                rx_load_chunk_requests,
                received_request: HashSet::default(),
                shared,
                runtime,
            }
            .run()
            .await;
        }
    });

    LaunchHandle {
        tx_load_chunk_requests,
    }
}

impl LaunchManager {
    async fn run(mut self) {
        while let Some(message) = self.rx_load_chunk_requests.recv().await {
            self.handle_load_chunk(message);
        }
    }

    fn handle_load_chunk(&mut self, message: Message) {
        let position = message.position;
        let newly_inserted = self.received_request.insert(position);

        if !newly_inserted {
            // people should already have a cached version of this chunk
            // or we are about to send it to them
            return;
        }

        let tx_load_chunks = message.tx;
        let shared = self.shared.clone();

        self.runtime.spawn(async move {
            let loaded_chunk = match load_chunk(position, &shared).await {
                Ok(loaded_chunk) => {
                    if loaded_chunk.chunk.height() == CHUNK_HEIGHT_SPAN {
                        loaded_chunk
                    } else {
                        warn!(
                            "got a chunk that did not have the correct height at {position}, \
                             setting to empty. This can happen if a chunk was generated in an old \
                             version of Minecraft."
                        );
                        empty_chunk(position)
                    }
                }
                Err(err) => {
                    warn!("failed to load chunk {position:?}: {err}");
                    empty_chunk(position)
                }
            };

            tx_load_chunks.send(loaded_chunk).unwrap();
        });
    }
}

fn empty_chunk(position: I16Vec2) -> LoadedChunk {
    // height: 24
    let unloaded = UnloadedChunk::with_height(CHUNK_HEIGHT_SPAN);

    let bytes = STATE.with_borrow_mut(|state| {
        encode_chunk_packet(&unloaded, position, state)
            .unwrap()
            .unwrap()
    });

    debug_assert_eq!(unloaded.height(), CHUNK_HEIGHT_SPAN);

    LoadedChunk::new(bytes.freeze(), unloaded, position)
}

async fn load_chunk(position: I16Vec2, shared: &Shared) -> anyhow::Result<LoadedChunk> {
    let x = i32::from(position.x);
    let y = i32::from(position.y);

    // todo: I do not love this heap allocation.
    let mut decompress_buf = vec![0; 1024 * 1024];

    // https://rust-lang.github.io/rust-clippy/master/index.html#/large_futures
    let Ok(region) = shared.regions.get_region_from_chunk(x, y).await else {
        // most likely the file representing the region does not exist so we will just return en empty chunk
        return Ok(empty_chunk(position));
    };

    let raw_chunk = {
        // todo: note that this is likely blocking to tokio
        region
            .get_chunk(x, y, &mut decompress_buf, shared.regions.root())?
            .context("no chunk found")?
    };

    let Ok(chunk) = parse_chunk(raw_chunk.data, &shared.biome_to_id) else {
        bail!("failed to parse chunk {position:?}");
    };

    STATE.with_borrow_mut(|state| {
        let Ok(Some(bytes)) = encode_chunk_packet(&chunk, position, state) else {
            bail!("failed to encode chunk {position:?}");
        };

        let loaded_chunk = LoadedChunk::new(bytes.freeze(), chunk, position);

        Ok(loaded_chunk)
    })
}

// #[instrument(skip_all, level = "trace", fields(location = ?location))]
fn encode_chunk_packet(
    chunk: &UnloadedChunk,
    location: I16Vec2,
    state: &mut TasksState,
) -> anyhow::Result<Option<BytesMut>> {
    let encoder = PacketEncoder::new(CompressionThreshold::from(6));

    let section_count = 384 / 16_usize;
    let dimension_height = 384;

    let map = heightmap(dimension_height, dimension_height - 3);
    let map = map.into_iter().map(i64::try_from).try_collect()?;

    // convert section_count + 2 0b1s into `u64` array
    let mut bits = BitStorage::new(1, section_count + 2, None).unwrap();

    for i in 0..section_count + 2 {
        bits.set(i, 1);
    }

    // 2048 bytes per section -> long count = 2048 / 8 = 256
    let sky_light_array = FixedArray([0xFF_u8; 2048]);
    let sky_light_arrays = vec![sky_light_array; section_count + 2];

    let mut section_bytes = Vec::new();

    for section in &chunk.sections {
        use valence_protocol::Encode;
        let non_air_blocks: u16 = 42;
        non_air_blocks.encode(&mut section_bytes).unwrap();

        write_block_states(&section.block_states, &mut section_bytes).unwrap();
        write_biomes(&section.biomes, &mut section_bytes).unwrap();
    }

    let pkt = play::ChunkDataS2c {
        pos: ChunkPos::new(i32::from(location.x), i32::from(location.y)),
        heightmaps: Cow::Owned(compound! {
            "MOTION_BLOCKING" => List::Long(map),
        }),
        blocks_and_biomes: &section_bytes,
        block_entities: Cow::Borrowed(&[]),

        sky_light_mask: Cow::Owned(bits.into_data()),
        block_light_mask: Cow::Borrowed(&[]),
        empty_sky_light_mask: Cow::Borrowed(&[]),
        empty_block_light_mask: Cow::Borrowed(&[]),
        sky_light_arrays: Cow::Owned(sky_light_arrays),
        block_light_arrays: Cow::Borrowed(&[]),
    };

    let buf = &mut state.bytes;
    let scratch = &mut state.scratch;
    let compressor = &mut state.compressor;

    let result = encoder.append_packet(&pkt, buf, scratch, compressor)?;

    Ok(Some(result))
}

fn write_block_states(states: &BlockStateContainer, writer: &mut impl Write) -> anyhow::Result<()> {
    states.encode_mc_format(
        writer,
        |b| b.to_raw().into(),
        4,
        8,
        bit_width(BlockState::max_raw().into()),
    )?;
    Ok(())
}

fn write_biomes(biomes: &BiomeContainer, writer: &mut impl Write) -> anyhow::Result<()> {
    biomes.encode_mc_format(
        writer,
        |b| b.to_index() as u64,
        0,
        3,
        6, // bit_width(info.biome_registry_len - 1),
    )?;
    Ok(())
}
