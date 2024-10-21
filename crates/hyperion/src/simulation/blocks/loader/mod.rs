use std::{borrow::Cow, cell::RefCell, io::Write, sync::Arc};

use anyhow::{bail, Context};
use bytes::BytesMut;
use glam::IVec2;
use itertools::Itertools;
use libdeflater::{CompressionLvl, Compressor};
use parse::ChunkData;
use rustc_hash::FxHashSet;
use tracing::warn;
use valence_generated::block::BlockState;
use valence_nbt::{compound, List};
use valence_protocol::{packets::play, ChunkPos, CompressionThreshold, FixedArray};
use valence_registry::RegistryIdx;
use valence_server::layer::chunk::{bit_width, BiomeContainer, Chunk};

pub mod parse;

use super::{chunk::LoadedChunk, shared::Shared};
use crate::{
    net::encoder::PacketEncoder, runtime::AsyncRuntime, simulation::util::heightmap,
    storage::BitStorage, Scratch, CHUNK_HEIGHT_SPAN,
};

struct TasksState {
    bytes: BytesMut,
    compressor: Compressor,
    scratch: Scratch,
}

impl Default for TasksState {
    fn default() -> Self {
        Self {
            bytes: BytesMut::new(),
            compressor: Compressor::new(CompressionLvl::new(1).unwrap()),
            scratch: Scratch::default(),
        }
    }
}

thread_local! {
  static STATE: RefCell<TasksState> = RefCell::new(TasksState::default());
}

struct Message {
    position: IVec2,
    tx: tokio::sync::mpsc::UnboundedSender<LoadedChunk>,
}

struct LaunchManager {
    rx_load_chunk_requests: tokio::sync::mpsc::UnboundedReceiver<Message>,
    received_request: FxHashSet<IVec2>,
    shared: Arc<Shared>,
    runtime: AsyncRuntime,
}

pub struct LaunchHandle {
    tx_load_chunk_requests: tokio::sync::mpsc::UnboundedSender<Message>,
}

impl LaunchHandle {
    pub fn send(&self, position: IVec2, tx: tokio::sync::mpsc::UnboundedSender<LoadedChunk>) {
        self.tx_load_chunk_requests
            .send(Message { position, tx })
            .unwrap();
    }
}

pub fn launch_manager(shared: Arc<Shared>, runtime: &AsyncRuntime) -> LaunchHandle {
    let (tx_load_chunk_requests, rx_load_chunk_requests) = tokio::sync::mpsc::unbounded_channel();

    tokio::task::Builder::new()
        .name("launch_manager")
        .spawn_on(
            {
                let runtime = runtime.clone();
                async move {
                    LaunchManager {
                        rx_load_chunk_requests,
                        received_request: FxHashSet::default(),
                        shared,
                        runtime,
                    }
                    .run()
                    .await;
                }
            },
            runtime.handle(),
        )
        .unwrap();

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
                    let chunk_height = loaded_chunk.chunk.height();
                    if chunk_height == CHUNK_HEIGHT_SPAN {
                        loaded_chunk
                    } else {
                        warn!(
                            "got a chunk that did not have the correct height at {position}, \
                             setting to empty. This can happen if a chunk was generated in an old \
                             version of Minecraft.\n\nExpected height: {CHUNK_HEIGHT_SPAN}, got \
                             {chunk_height}"
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

fn empty_chunk(position: IVec2) -> LoadedChunk {
    // height: 24
    let unloaded = ChunkData::with_height(CHUNK_HEIGHT_SPAN);

    let bytes = STATE.with_borrow_mut(|state| {
        encode_chunk_packet(&unloaded, position, state)
            .unwrap()
            .unwrap()
    });

    debug_assert_eq!(unloaded.height(), CHUNK_HEIGHT_SPAN);

    LoadedChunk::new(bytes.freeze(), unloaded, position)
}

async fn load_chunk(position: IVec2, shared: &Shared) -> anyhow::Result<LoadedChunk> {
    let x = position.x;
    let y = position.y;

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

    let chunk = match parse::parse_chunk(raw_chunk.data, &shared.biome_to_id) {
        Ok(chunk) => chunk,
        Err(err) => {
            bail!("failed to parse chunk {position:?}: {err}");
        }
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
    chunk: &ChunkData,
    location: IVec2,
    state: &mut TasksState,
) -> anyhow::Result<Option<BytesMut>> {
    let encoder = PacketEncoder::new(CompressionThreshold::from(6));

    let section_count = CHUNK_HEIGHT_SPAN as usize / 16_usize;
    let dimension_height = CHUNK_HEIGHT_SPAN;

    let map = heightmap(dimension_height, dimension_height - 3);
    let map = map.into_iter().map(i64::try_from).try_collect()?;

    // convert section_count + 2 0b1s into `u64` array
    // todo: this is jank let's do the non jank way so we can get smaller packet sizes
    let mut all_ones = BitStorage::new(1, section_count + 2, None)?;

    for i in 0..section_count + 2 {
        all_ones.set(i, 1);
    }

    // 2048 bytes per section -> long count = 2048 / 8 = 256
    let empty_light = FixedArray([0x00_u8; 2048]);

    let mut sky_light_arrays = vec![empty_light];
    let mut block_light_arrays = vec![empty_light];

    let mut section_bytes = Vec::new();

    for section in &chunk.sections {
        use valence_protocol::Encode;
        let non_air_blocks: u16 = 42;
        non_air_blocks.encode(&mut section_bytes).unwrap();

        // todo: how do sky light and block light work differently?

        let sky_light = section.sky_light;
        let sky_light = FixedArray(sky_light);

        sky_light_arrays.push(sky_light);

        let block_light = section.block_light;
        let block_light = FixedArray(block_light);

        block_light_arrays.push(block_light);

        write_block_states(&section.block_states, &mut section_bytes).unwrap();
        write_biomes(&section.biomes, &mut section_bytes).unwrap();
    }

    // todo: Maybe we want the top one to actually be all Fs because I think this is just an edge case for how things are rendered.
    sky_light_arrays.push(empty_light);
    block_light_arrays.push(empty_light);

    debug_assert_eq!(sky_light_arrays.len(), section_count + 2);

    let ones_data = all_ones.into_data();

    let pkt = play::ChunkDataS2c {
        pos: ChunkPos::new(location.x, location.y),

        // todo: I think this is for rain and snow???
        heightmaps: Cow::Owned(compound! {
            "MOTION_BLOCKING" => List::Long(map),
        }),
        blocks_and_biomes: &section_bytes,
        block_entities: Cow::Borrowed(&[]),

        sky_light_mask: Cow::Borrowed(&ones_data),
        block_light_mask: Cow::Borrowed(&ones_data),

        empty_sky_light_mask: Cow::Borrowed(&[]),
        empty_block_light_mask: Cow::Borrowed(&[]),

        sky_light_arrays: Cow::Owned(sky_light_arrays),
        block_light_arrays: Cow::Owned(block_light_arrays),
    };

    let buf = &mut state.bytes;
    let scratch = &mut state.scratch;
    let compressor = &mut state.compressor;

    let result = encoder.append_packet(&pkt, buf, scratch, compressor)?;

    Ok(Some(result))
}

fn write_block_states(
    states: &hyperion_palette::PalettedContainer,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    states.encode_mc_format(
        writer,
        derive_more::Into::into,
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
