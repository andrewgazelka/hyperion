//! Constructs for working with blocks.

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    io::Write,
    ops::Try,
    sync::Arc,
};

use anyhow::Context;
use bytes::{Bytes, BytesMut};
use dashmap::{DashMap, DashSet};
use flecs_ecs::macros::Component;
use fxhash::FxBuildHasher;
use glam::{I16Vec2, IVec2};
use itertools::Itertools;
use libdeflater::{CompressionLvl, Compressor};
use tokio::task::JoinHandle;
use tracing::error;
use valence_anvil::parsing::parse_chunk;
use valence_generated::block::BlockState;
use valence_nbt::{compound, List};
use valence_protocol::{
    packets::play, BlockPos, ChunkPos, CompressionThreshold, Encode, FixedArray, Ident,
};
use valence_registry::{biome::BiomeId, BiomeRegistry, RegistryIdx};
use valence_server::layer::chunk::{
    bit_width, BiomeContainer, BlockStateContainer, Chunk, UnloadedChunk,
};

use crate::{
    bits::BitStorage, chunk::heightmap, component::blocks::loader::Regions,
    net::encoder::PacketEncoder, runtime::AsyncRuntime, Scratch,
};

mod loader;
mod region;

/// Thread-local state for encoding chunks.
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

/// A chunk which has been loaded into memory.
#[derive(Debug)]
pub struct LoadedChunk {
    /// The raw (usually compressed) bytes of the chunk that are sent to the client via the Minecraft protocol.
    pub packet_bytes: Bytes,

    /// The actual chunk data that is "uncompressed". It uses a palette to store the actual data. This is usually used
    /// for obtaining the actual data from the chunk such as getting the block state of a block at a given position.
    pub chunk: Option<UnloadedChunk>,
}

/// Accessor of blocks.
#[derive(Component)]
pub struct Blocks {
    threaded: Arc<BlocksInner>,
    cache: HashMap<I16Vec2, LoadedChunk, FxBuildHasher>,
}

/// Inner state of the [`Blocks`] component.
pub struct BlocksInner {
    // todo: impl more efficient (probably lru) cache
    pending_cache: DashMap<I16Vec2, LoadedChunk, FxBuildHasher>,
    loading: DashSet<I16Vec2, FxBuildHasher>,
    regions: Regions,
    biome_to_id: BTreeMap<Ident<String>, BiomeId>,
}

impl BlocksInner {
    pub(crate) fn new(biomes: &BiomeRegistry) -> anyhow::Result<Self> {
        let regions = Regions::new().context("failed to get anvil data")?;

        let biome_to_id = biomes
            .iter()
            .map(|(id, name, _)| (name.to_string_ident(), id))
            .collect();

        Ok(Self {
            pending_cache: DashMap::default(),
            loading: DashSet::default(),
            regions,
            biome_to_id,
        })
    }
}

thread_local! {
  static STATE: RefCell<TasksState> = RefCell::new(TasksState::default());
}

/// The current data of a chunk.
pub enum ChunkData {
    /// The chunk has been loaded into memory and is cached.
    Cached(Bytes),
    /// The chunks is currently being processed and loaded into memory.
    Task(JoinHandle<()>),
}

impl Blocks {
    pub(crate) fn new(registry: &BiomeRegistry) -> anyhow::Result<Self> {
        let inner = BlocksInner::new(registry)?;
        Ok(Self {
            threaded: Arc::new(inner),
            cache: HashMap::default(),
        })
    }

    /// todo: doesn't work in loading state
    #[allow(clippy::missing_panics_doc, reason = "todo use unwrap unchecked")]
    pub fn get_and_wait(
        &self,
        position: I16Vec2,
        tasks: &AsyncRuntime,
    ) -> anyhow::Result<Option<Bytes>> {
        let result = match self.get_cached_or_load(position, tasks)? {
            None => None,
            Some(ChunkData::Cached(data)) => Some(data),
            Some(ChunkData::Task(handle)) => {
                tasks.block_on(handle)?;
                let res = self
                    .threaded
                    .pending_cache
                    .get(&position)
                    .unwrap()
                    .packet_bytes
                    .clone();
                Some(res)
            }
        };

        Ok(result)
    }

    pub fn load_pending(&mut self) {
        let keys: Vec<_> = self
            .threaded
            .pending_cache
            .iter()
            .map(|x| *x.key())
            .collect();

        for key in keys {
            let (_, v) = self.threaded.pending_cache.remove(&key).unwrap();
            self.cache.insert(key, v);
        }
    }

    /// Returns the unloaded chunk if it is loaded, otherwise `None`.
    // todo: return type: what do you think about the type right here?
    // This seems really complicated.
    // I wonder if we can just implement something, where we can return an `impl Deref`
    // and see if this would make more sense or not.
    #[must_use]

    pub fn get_loaded_chunk(&self, chunk_position: I16Vec2) -> Option<&UnloadedChunk> {
        self.cache
            .get(&chunk_position)
            .and_then(|x| x.chunk.as_ref())
    }

    /// Returns all loaded blocks within the range from `start` to `end` (inclusive).
    pub fn get_blocks<F, R>(&self, start: BlockPos, end: BlockPos, mut f: F) -> R
    where
        F: FnMut(BlockPos, BlockState) -> R,
        R: Try<Output = ()>,
    {
        const START_Y: i32 = -64;

        let start_xz = IVec2::new(start.x, start.z);
        let end_xz = IVec2::new(end.x, end.z);

        let start_chunk_pos: IVec2 = start_xz >> 4;
        let end_chunk_pos: IVec2 = end_xz >> 4;

        // let start_chunk_pos = start_chunk_pos.as_i16vec2();
        // let end_chunk_pos = end_chunk_pos.as_i16vec2();

        #[allow(clippy::cast_sign_loss)]
        let y_start = (start.y - START_Y).max(0) as u32;

        #[allow(clippy::cast_sign_loss)]
        let y_end = (end.y - START_Y).max(0) as u32;

        for cx in start_chunk_pos.x..=end_chunk_pos.x {
            for cz in start_chunk_pos.y..=end_chunk_pos.y {
                let chunk_start = IVec2::new(cx, cz) << 4;
                let chunk_end = chunk_start + IVec2::splat(15);

                let start = start_xz.clamp(chunk_start, chunk_end);
                let end = end_xz.clamp(chunk_start, chunk_end);

                debug_assert!(start.x >= start_xz.x);
                debug_assert!(start.y >= start_xz.y);
                debug_assert!(end.x <= end_xz.x);
                debug_assert!(end.y <= end_xz.y);

                let start = start & 0b1111;
                let end = end & 0b1111;

                debug_assert!(start.x >= 0, "start = {start}");
                debug_assert!(start.y >= 0, "start = {start}");
                debug_assert!(start.x <= 15, "start = {start}");
                debug_assert!(start.y <= 15, "start = {start}");

                debug_assert!(end.x >= 0);
                debug_assert!(end.y >= 0);
                debug_assert!(end.x <= 15);
                debug_assert!(end.y <= 15);

                debug_assert!(start.x <= end.x);
                debug_assert!(start.y <= end.y);

                let start = start.as_uvec2();
                let end = end.as_uvec2();

                let chunk_pos = IVec2::new(cx, cz).as_i16vec2();

                let Some(chunk) = self.get_loaded_chunk(chunk_pos) else {
                    continue;
                };

                for x in start.x..=end.x {
                    for z in start.y..=end.y {
                        for y in y_start..=y_end {
                            debug_assert!(x <= 15);
                            debug_assert!(z <= 15);

                            let block = chunk.block_state(x, y, z);

                            let y = y as i32 + START_Y;
                            let pos = BlockPos::new(
                                x as i32 + chunk_start.x,
                                y,
                                z as i32 + chunk_start.y,
                            );

                            f(pos, block)?;
                        }
                    }
                }
            }
        }

        R::from_output(())
    }

    /// Get a block
    #[must_use]
    pub fn get_block(&self, position: BlockPos) -> Option<BlockState> {
        const START_Y: i32 = -64;

        if position.y < START_Y {
            // This block is in the void.
            return Some(BlockState::VOID_AIR);
        }

        let chunk_pos: IVec2 = IVec2::new(position.x, position.z) >> 4;
        let chunk_start_block: IVec2 = chunk_pos << 4;
        let chunk_pos = chunk_pos.as_i16vec2();

        let chunk = self.get_loaded_chunk(chunk_pos)?;

        // todo: is this right for negative numbers?
        // I have no idea... let's test
        // non-absolute difference should work as well, but we want a u32
        let x = u32::try_from(position.x - chunk_start_block[0]).unwrap();
        let y = u32::try_from(position.y - START_Y).unwrap();
        let z = u32::try_from(position.z - chunk_start_block[1]).unwrap();

        Some(chunk.block_state(x, y, z))
    }

    // todo: allow modifying the chunk. we will need to implement resending
    // So,
    // for instance, if a player modifies a chunk, we're going to need to rebroadcast it to all the players in that region.
    // However, I'm going to wait until my broadcasting code using the new proxy is done before I do this.
    // If you want to implement this, I also recommend waiting until that's done.
    // That should be done in a couple of days, probably.

    // #[instrument(skip_all, level = "trace")]
    /// get the cached chunk for the given position or load it if it is not cached.
    pub fn get_cached_or_load(
        &self,
        position: I16Vec2,
        tasks: &AsyncRuntime,
    ) -> anyhow::Result<Option<ChunkData>> {
        if let Some(result) = self.cache.get(&position) {
            return Ok(Some(ChunkData::Cached(result.packet_bytes.clone())));
        }

        if !self.threaded.loading.insert(position) {
            // we are currently loading this chunk
            return Ok(None);
        }

        let inner = self.threaded.clone();

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
                inner.pending_cache.insert(position, LoadedChunk {
                    packet_bytes: Bytes::new(),
                    chunk: None,
                });

                inner.loading.remove(&position);

                return;
            };

            STATE.with_borrow_mut(|state| {
                let Ok(Some(bytes)) = encode_chunk_packet(&chunk, position, state) else {
                    inner.pending_cache.insert(position, LoadedChunk {
                        packet_bytes: Bytes::new(),
                        chunk: None,
                    });

                    inner.loading.remove(&position);
                    return;
                };

                inner.pending_cache.insert(position, LoadedChunk {
                    packet_bytes: bytes.freeze(),
                    chunk: Some(chunk),
                });

                let present = inner.loading.remove(&position);

                debug_assert!(present.is_some());
            });
        });

        Ok(Some(ChunkData::Task(handle)))
    }
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
