use std::{borrow::Cow, cell::RefCell, collections::BTreeMap, io::Write, sync::Arc};

use anyhow::Context;
use bytes::{Bytes, BytesMut};
use dashmap::{DashMap, DashSet};
use derive_more::{Deref, DerefMut};
use evenio::component::Component;
use fxhash::FxBuildHasher;
use glam::I16Vec2;
use itertools::Itertools;
use libdeflater::{CompressionLvl, Compressor};
use tokio::{runtime::Runtime, task::JoinHandle};
use tracing::{error, instrument};
use valence_anvil::parsing::parse_chunk;
use valence_generated::block::BlockState;
use valence_nbt::{compound, List};
use valence_protocol::{packets::play, ChunkPos, CompressionThreshold, Encode, FixedArray, Ident};
use valence_registry::{biome::BiomeId, BiomeRegistry, RegistryIdx};
use valence_server::layer::chunk::{bit_width, BiomeContainer, BlockStateContainer, UnloadedChunk};

use crate::{
    bits::BitStorage, chunk::heightmap, components::chunks::loader::Regions, default,
    event::Scratch, net::encoder::PacketEncoder,
};
mod loader;
mod region;

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

#[derive(Component, Deref, DerefMut)]
pub struct Tasks {
    runtime: Runtime,
}

impl Default for Tasks {
    fn default() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self { runtime }
    }
}

#[derive(Debug)]
pub struct LoadedChunk {
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
    cache: DashMap<I16Vec2, LoadedChunk, FxBuildHasher>,
    loading: DashSet<I16Vec2, FxBuildHasher>,
    regions: Regions,
    biome_to_id: BTreeMap<Ident<String>, BiomeId>,
}

impl ChunksInner {}

impl ChunksInner {
    pub fn new(biomes: &BiomeRegistry) -> anyhow::Result<Self> {
        let regions = Regions::new().context("failed to get anvil data")?;

        let biome_to_id = biomes
            .iter()
            .map(|(id, name, _)| (name.to_string_ident(), id))
            .collect();

        Ok(Self {
            cache: default(),
            loading: default(),
            regions,
            biome_to_id,
        })
    }
}

thread_local! {
  static STATE: RefCell<TasksState> = RefCell::new(TasksState::default());
}

pub enum ChunkData {
    Cached(Bytes),
    Task(JoinHandle<()>),
}

impl Chunks {
    /// todo: doesn't work in loading state
    #[allow(clippy::missing_panics_doc, reason = "todo use unwrap unchecked")]
    pub fn get_and_wait(&self, position: I16Vec2, tasks: &Tasks) -> anyhow::Result<Option<Bytes>> {
        let result = match self.get_cached_or_load(position, tasks)? {
            None => None,
            Some(ChunkData::Cached(data)) => Some(data),
            Some(ChunkData::Task(handle)) => {
                tasks.block_on(handle)?;
                let res = self.inner.cache.get(&position).unwrap().raw.clone();
                Some(res)
            }
        };

        Ok(result)
    }

    #[instrument(skip_all, level = "trace")]
    pub fn get_cached_or_load(
        &self,
        position: I16Vec2,
        tasks: &Tasks,
    ) -> anyhow::Result<Option<ChunkData>> {
        if let Some(result) = self.inner.cache.get(&position) {
            return Ok(Some(ChunkData::Cached(result.raw.clone())));
        }

        if !self.inner.loading.insert(position) {
            return Ok(None);
        }

        let inner = self.inner.clone();

        let handle = tasks.spawn(async move {
            let mut decompress_buf = vec![0; 1024 * 1024];

            // https://rust-lang.github.io/rust-clippy/master/index.html#/large_futures
            let region = inner
                .regions
                .get_region_from_chunk(i32::from(position.x), i32::from(position.y))
                .await;

            let raw_chunk = {
                let mut region_access = region.lock().await;

                region_access
                    .get_chunk(
                        i32::from(position.x),
                        i32::from(position.y),
                        &mut decompress_buf,
                        inner.regions.root(),
                    )
                    .await
                    .unwrap()
                    .unwrap()
            };

            let Ok(chunk) = parse_chunk(raw_chunk.data, &inner.biome_to_id) else {
                error!("failed to parse chunk {position:?}");
                inner
                    .cache
                    .insert(position, LoadedChunk { raw: Bytes::new() });

                inner.loading.remove(&position);

                return;
            };

            STATE.with_borrow_mut(|state| {
                let Ok(Some(bytes)) = encode_chunk_packet(&chunk, position, state) else {
                    inner
                        .cache
                        .insert(position, LoadedChunk { raw: Bytes::new() });

                    inner.loading.remove(&position);
                    return;
                };

                inner.cache.insert(position, LoadedChunk {
                    raw: bytes.freeze(),
                });

                let present = inner.loading.remove(&position);

                debug_assert!(present.is_some());
            });
        });

        Ok(Some(ChunkData::Task(handle)))
    }
}

#[instrument(skip_all, level = "trace", fields(location = ?location))]
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
