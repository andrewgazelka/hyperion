use std::{borrow::Cow, cell::RefCell, io::Write, sync::Arc};

use anyhow::Context;
use bytes::{Bytes, BytesMut};
use dashmap::{DashMap, DashSet};
use derive_more::{Deref, DerefMut};
use evenio::component::Component;
use fxhash::FxBuildHasher;
use itertools::Itertools;
use libdeflater::{CompressionLvl, Compressor};
use switchyard::{threads::thread_info, Switchyard};
use tracing::instrument;
use valence_generated::block::BlockState;
use valence_nbt::{compound, List};
use valence_protocol::{packets::play, ChunkPos, CompressionThreshold, Encode, FixedArray};
use valence_registry::{BiomeRegistry, RegistryIdx};
use valence_server::layer::chunk::{bit_width, BiomeContainer, BlockStateContainer, UnloadedChunk};

use crate::{
    bits::BitStorage, blocks::AnvilFolder, chunk::heightmap, default, event::Scratch,
    net::encoder::PacketEncoder,
};

pub struct TasksState {
    bytes: BytesMut,
    scratch: Scratch,
    compressor: Compressor,
}

impl Default for TasksState {
    fn default() -> Self {
        Self {
            bytes: BytesMut::new(),
            scratch: Scratch::default(),
            compressor: Compressor::new(CompressionLvl::new(6).unwrap()),
        }
    }
}

#[derive(Deref, DerefMut, Component)]
pub struct Tasks {
    switchyard: Switchyard<RefCell<TasksState>>,
}

impl Default for Tasks {
    fn default() -> Self {
        let allocations = switchyard::threads::one_to_one(thread_info(), Some("switchyard"));

        let switchyard =
            Switchyard::new(allocations, || RefCell::new(TasksState::default())).unwrap();

        Self { switchyard }
    }
}

#[derive(Debug)]
pub struct LoadedChunk {
    // inner: UnloadedChunk,
    pub raw: Bytes,
}

#[derive(Component)]
pub struct Chunks {
    inner: Arc<ChunksInner>,
}

impl Chunks {
    pub fn new(registry: &BiomeRegistry) -> anyhow::Result<Self> {
        let inner = ChunksInner::new(registry)?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }
}

pub struct ChunksInner {
    // todo: impl more efficient (probably lru) cache
    cache: DashMap<ChunkPos, LoadedChunk, FxBuildHasher>,
    loading: DashSet<ChunkPos, FxBuildHasher>,
    loader: parking_lot::Mutex<AnvilFolder>,
}

impl ChunksInner {
    pub fn new(registry: &BiomeRegistry) -> anyhow::Result<Self> {
        let loader = AnvilFolder::new(registry).context("failed to get anvil data")?;
        Ok(Self {
            cache: default(),
            loading: default(),
            loader: loader.into(),
        })
    }
}

impl Chunks {
    #[instrument(skip_all, level = "trace")]
    pub fn get_cached_or_load(
        &self,
        position: ChunkPos,
        tasks: &Tasks,
    ) -> anyhow::Result<Option<Bytes>> {
        if let Some(result) = self.inner.cache.get(&position) {
            return Ok(Some(result.raw.clone()));
        }

        if !self.inner.loading.insert(position) {
            return Ok(None);
        }

        let inner = self.inner.clone();

        tasks.spawn_local(1, move |state| async move {
            let chunk = {
                let mut loader = inner.loader.lock();
                loader.dim.get_chunk(position)
            };

            let Ok(chunk) = chunk else {
                return;
            };

            let Some(chunk) = chunk else {
                return;
            };

            let mut state = state.borrow_mut();
            let Ok(Some(bytes)) = encode_chunk_packet(&chunk.chunk, position, &mut state) else {
                return;
            };

            inner.cache.insert(position, LoadedChunk {
                raw: bytes.freeze(),
            });

            let present = inner.loading.remove(&position);

            debug_assert!(present.is_some());
        });

        Ok(None)
    }
}

#[instrument(skip_all, level = "trace", fields(location = ?location))]
fn encode_chunk_packet(
    chunk: &UnloadedChunk,
    location: ChunkPos,
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
        let non_air_blocks: u16 = 42;
        non_air_blocks.encode(&mut section_bytes).unwrap();

        write_block_states(&section.block_states, &mut section_bytes).unwrap();
        write_biomes(&section.biomes, &mut section_bytes).unwrap();
    }

    let pkt = play::ChunkDataS2c {
        pos: location,
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
