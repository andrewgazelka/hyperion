//! Constructs for working with blocks.

use std::{ops::Try, sync::Arc};

use bytes::Bytes;
use chunk::LoadedChunk;
use flecs_ecs::{core::Entity, macros::Component};
use fxhash::FxBuildHasher;
use glam::{I16Vec2, IVec2};
use indexmap::IndexMap;
use loader::{launch_manager, LaunchHandle, CHUNK_HEIGHT_SPAN};
use roaring::RoaringBitmap;
use shared::Shared;
use tracing::instrument;
use valence_generated::block::BlockState;
use valence_protocol::BlockPos;
use valence_registry::BiomeRegistry;
use valence_server::layer::chunk::Chunk;

use crate::runtime::AsyncRuntime;

pub mod chunk;

mod loader;
mod manager;

pub mod frame;
mod region;
mod shared;

pub enum GetChunk<'a> {
    Loaded(&'a LoadedChunk),
    Loading,
}

pub struct EntityAndSequence {
    pub entity: Entity,
    pub sequence: i32,
}

#[derive(Debug)]
pub enum TrySetBlockDeltaError {
    OutOfBounds,
    ChunkNotLoaded,
}

/// Accessor of blocks.
#[derive(Component)]
pub struct MinecraftWorld {
    /// Map to a Chunk by Entity ID
    chunk_cache: IndexMap<I16Vec2, LoadedChunk, FxBuildHasher>,
    should_update: RoaringBitmap,

    launch_manager: LaunchHandle,

    tx_loaded_chunks: tokio::sync::mpsc::UnboundedSender<LoadedChunk>,
    rx_loaded_chunks: tokio::sync::mpsc::UnboundedReceiver<LoadedChunk>,
    pub to_confirm: Vec<EntityAndSequence>,
}

impl MinecraftWorld {
    pub(crate) fn new(registry: &BiomeRegistry, runtime: AsyncRuntime) -> anyhow::Result<Self> {
        let shared = Shared::new(registry, &runtime)?;
        let shared = Arc::new(shared);

        let (tx_loaded_chunks, rx_loaded_chunks) = tokio::sync::mpsc::unbounded_channel();

        Ok(Self {
            chunk_cache: IndexMap::default(),
            should_update: RoaringBitmap::default(),
            launch_manager: launch_manager(shared, runtime),
            tx_loaded_chunks,
            rx_loaded_chunks,
            to_confirm: vec![],
        })
    }

    pub fn for_each_to_update_mut(&mut self, mut f: impl FnMut(&mut LoadedChunk)) {
        let should_update = &mut self.should_update;
        let chunk_cache = &mut self.chunk_cache;

        for idx in should_update.iter() {
            let idx = idx as usize;
            let (_, v) = chunk_cache.get_index_mut(idx).unwrap();
            f(v);
        }
    }

    pub fn for_each_to_update(&self, mut f: impl FnMut(&LoadedChunk)) {
        let should_update = &self.should_update;
        let chunk_cache = &self.chunk_cache;

        for idx in should_update.iter() {
            let idx = idx as usize;
            let (_, v) = chunk_cache.get_index(idx).unwrap();
            f(v);
        }
    }

    pub fn clear_should_update(&mut self) {
        self.should_update.clear();
    }

    pub fn cache_mut(&mut self) -> &mut IndexMap<I16Vec2, LoadedChunk, FxBuildHasher> {
        &mut self.chunk_cache
    }

    /// get_and_wait can only be called if a chunk has not already been loaded
    #[allow(clippy::missing_panics_doc, reason = "todo use unwrap unchecked")]
    #[instrument(skip_all)]
    pub unsafe fn get_and_wait(&self, position: I16Vec2, tasks: &AsyncRuntime) -> Bytes {
        if let Some(cached) = self.get_cached(position) {
            return cached;
        }

        // get_and_wait is called infrequently, ideally this would be a oneshot channel
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        // todo: potential race condition where this is called twice
        self.launch_manager.send(position, tx);

        let result = tasks.block_on(async move { rx.recv().await.unwrap() });

        let bytes = result.base_packet_bytes.clone();

        // forward to the main channel
        self.tx_loaded_chunks.send(result).unwrap();

        bytes
    }

    pub fn load_pending(&mut self) {
        while let Ok(chunk) = self.rx_loaded_chunks.try_recv() {
            let position = chunk.position;

            self.chunk_cache.insert(position, chunk);
        }
    }

    /// Returns the unloaded chunk if it is loaded, otherwise `None`.
    // todo: return type: what do you think about the type right here?
    // This seems really complicated.
    // I wonder if we can just implement something, where we can return an `impl Deref`
    // and see if this would make more sense or not.
    #[must_use]
    pub fn get_loaded_chunk(&self, chunk_position: I16Vec2) -> Option<&LoadedChunk> {
        self.chunk_cache.get(&chunk_position)
    }

    pub fn get_loaded_chunk_mut(&mut self, chunk_position: I16Vec2) -> Option<&mut LoadedChunk> {
        self.chunk_cache.get_mut(&chunk_position)
    }

    /// Returns all loaded blocks within the range from `start` to `end` (inclusive).
    #[allow(clippy::excessive_nesting)]
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

                let chunk = &chunk.chunk;
                for x in start.x..=end.x {
                    for z in start.y..=end.y {
                        for y in y_start..=y_end {
                            debug_assert!(x <= 15);
                            debug_assert!(z <= 15);

                            if y >= CHUNK_HEIGHT_SPAN {
                                continue;
                            }

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

        let chunk = &chunk.chunk;
        // todo: is this right for negative numbers?
        // I have no idea... let's test
        // non-absolute difference should work as well, but we want a u32
        let x = u32::try_from(position.x - chunk_start_block[0]).unwrap();
        let y = u32::try_from(position.y - START_Y).unwrap();
        let z = u32::try_from(position.z - chunk_start_block[1]).unwrap();

        Some(chunk.block_state(x, y, z))
    }

    /// Returns the old block state
    pub fn set_block(
        &mut self,
        position: BlockPos,
        state: BlockState,
    ) -> Result<BlockState, TrySetBlockDeltaError> {
        const START_Y: i32 = -64;

        if position.y < START_Y {
            // This block is in the void.
            // todo: do we want this to be error?
            return Err(TrySetBlockDeltaError::OutOfBounds);
        }

        let chunk_pos: IVec2 = IVec2::new(position.x, position.z) >> 4;
        let chunk_start_block: IVec2 = chunk_pos << 4;
        let chunk_pos = chunk_pos.as_i16vec2();

        let Some((chunk_idx, _, chunk)) = self.chunk_cache.get_full_mut(&chunk_pos) else {
            return Err(TrySetBlockDeltaError::ChunkNotLoaded);
        };

        let x = u32::try_from(position.x - chunk_start_block[0]).unwrap();
        let y = u32::try_from(position.y - START_Y).unwrap();
        let z = u32::try_from(position.z - chunk_start_block[1]).unwrap();

        let old_state = chunk.chunk.set_delta(x, y, z, state);

        if old_state != state {
            self.should_update.insert(chunk_idx as u32);
        }

        Ok(old_state)
    }

    // todo: allow modifying the chunk. we will need to implement resending
    // So,
    // for instance, if a player modifies a chunk, we're going to need to rebroadcast it to all the players in that region.
    // However, I'm going to wait until my broadcasting code using the new proxy is done before I do this.
    // If you want to implement this, I also recommend waiting until that's done.
    // That should be done in a couple of days, probably.

    pub fn get_cached(&self, position: I16Vec2) -> Option<Bytes> {
        if let Some(result) = self.chunk_cache.get(&position) {
            return Some(result.bytes());
        }

        None
    }

    /// get the cached chunk for the given position or load it if it is not cached.
    #[must_use]
    pub fn get_cached_or_load(&self, position: I16Vec2) -> GetChunk<'_> {
        if let Some(result) = self.chunk_cache.get(&position) {
            return GetChunk::Loaded(result);
        };

        self.launch_manager
            .send(position, self.tx_loaded_chunks.clone());

        GetChunk::Loading
    }
}
