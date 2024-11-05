//! Constructs for working with blocks.

use std::{future::Future, ops::Try, path::Path, pin::Pin, sync::Arc};

use anyhow::Context;
use bytes::Bytes;
use chunk::Column;
use flecs_ecs::{
    core::{Entity, World, WorldGet},
    macros::Component,
};
use glam::{IVec2, IVec3};
use indexmap::IndexMap;
use loader::{ChunkLoaderHandle, launch_manager};
use roaring::RoaringBitmap;
use rustc_hash::FxBuildHasher;
use shared::WorldShared;
use tracing::error;
use valence_generated::block::BlockState;
use valence_registry::BiomeRegistry;
use valence_server::layer::chunk::Chunk;

use crate::{CHUNK_HEIGHT_SPAN, runtime::AsyncRuntime, simulation::util::generate_biome_registry};

pub mod chunk;

mod loader;
mod manager;

pub mod frame;
mod region;
mod shared;

pub enum GetChunk<'a> {
    Loaded(&'a Column),
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
pub struct Blocks {
    /// Map to a Chunk by Entity ID
    chunk_cache: IndexMap<IVec2, Column, FxBuildHasher>,
    should_update: RoaringBitmap,

    loader_handle: ChunkLoaderHandle,

    tx_loaded_chunks: tokio::sync::mpsc::UnboundedSender<Column>,
    rx_loaded_chunks: tokio::sync::mpsc::UnboundedReceiver<Column>,
    pub to_confirm: Vec<EntityAndSequence>,
}

impl Blocks {
    pub fn new(world: &World, path: &Path) -> anyhow::Result<Self> {
        world.get::<&AsyncRuntime>(|runtime| {
            let biome_registry =
                generate_biome_registry().context("failed to generate biome registry")?;

            let shared = WorldShared::new(&biome_registry, runtime, path)?;
            let shared = Arc::new(shared);

            let (tx_loaded_chunks, rx_loaded_chunks) = tokio::sync::mpsc::unbounded_channel();

            Ok(Self {
                chunk_cache: IndexMap::default(),
                should_update: RoaringBitmap::default(),
                loader_handle: launch_manager(shared, runtime),
                tx_loaded_chunks,
                rx_loaded_chunks,
                to_confirm: vec![],
            })
        })
    }

    pub fn for_each_to_update_mut(&mut self, mut f: impl FnMut(&mut Column)) {
        let should_update = &mut self.should_update;
        let chunk_cache = &mut self.chunk_cache;

        for idx in should_update.iter() {
            let idx = idx as usize;
            let (_, v) = chunk_cache.get_index_mut(idx).unwrap();
            f(v);
        }
    }

    pub fn for_each_to_update(&self, mut f: impl FnMut(&Column)) {
        let should_update = &self.should_update;
        let chunk_cache = &self.chunk_cache;

        for idx in should_update {
            let idx = idx as usize;
            let (_, v) = chunk_cache.get_index(idx).unwrap();
            f(v);
        }
    }

    pub fn clear_should_update(&mut self) {
        self.should_update.clear();
    }

    pub fn cache_mut(&mut self) -> &mut IndexMap<IVec2, Column, FxBuildHasher> {
        &mut self.chunk_cache
    }

    pub fn block_and_load(&mut self, column_location: IVec2, tasks: &AsyncRuntime) {
        tasks.block_on(async { self.get_and_wait(column_location).await });

        self.load_pending();
    }

    #[must_use]
    pub fn get_and_wait(&self, position: IVec2) -> Pin<Box<dyn Future<Output = Bytes> + Send>> {
        if let Some(cached) = self.get_cached(position) {
            return Box::pin(core::future::ready(cached));
        }

        // get_and_wait is called infrequently, ideally this would be a oneshot channel
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        // todo: potential race condition where this is called twice
        self.loader_handle.send(position, tx);

        let blocks_tx = self.tx_loaded_chunks.clone();

        let result = async move {
            let Some(result) = rx.recv().await else {
                error!("failed to get chunk from cache");
                return Bytes::new();
            };

            let bytes = result.base_packet_bytes.clone();

            // forward to the main channel
            blocks_tx.send(result).unwrap();

            bytes
        };

        Box::pin(result)
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
    pub fn get_loaded_chunk(&self, chunk_position: IVec2) -> Option<&Column> {
        self.chunk_cache.get(&chunk_position)
    }

    pub fn get_loaded_chunk_mut(&mut self, chunk_position: IVec2) -> Option<&mut Column> {
        self.chunk_cache.get_mut(&chunk_position)
    }

    /// Returns all loaded blocks within the range from `start` to `end` (inclusive).
    #[expect(clippy::excessive_nesting)]
    pub fn get_blocks<F, R>(&self, start: IVec3, end: IVec3, mut f: F) -> R
    where
        F: FnMut(IVec3, BlockState) -> R,
        R: Try<Output = ()>,
    {
        const START_Y: i32 = -64;

        let start_xz = IVec2::new(start.x, start.z);
        let end_xz = IVec2::new(end.x, end.z);

        let start_chunk_pos: IVec2 = start_xz >> 4;
        let end_chunk_pos: IVec2 = end_xz >> 4;

        // let start_chunk_pos = start_chunk_pos.as_i16vec2();
        // let end_chunk_pos = end_chunk_pos.as_i16vec2();

        #[expect(clippy::cast_sign_loss)]
        let y_start = (start.y - START_Y).max(0) as u32;

        #[expect(clippy::cast_sign_loss)]
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

                let chunk_pos = IVec2::new(cx, cz);

                let Some(chunk) = self.get_loaded_chunk(chunk_pos) else {
                    continue;
                };

                let chunk = &chunk.data;
                for x in start.x..=end.x {
                    for z in start.y..=end.y {
                        for y in y_start..=y_end {
                            debug_assert!(x <= 15);
                            debug_assert!(z <= 15);

                            if y >= CHUNK_HEIGHT_SPAN {
                                continue;
                            }

                            let block = chunk.block_state(x, y, z);

                            let y = i32::try_from(y).unwrap() + START_Y;
                            let pos = IVec3::new(
                                i32::try_from(x).unwrap() + chunk_start.x,
                                y,
                                i32::try_from(z).unwrap() + chunk_start.y,
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
    pub fn get_block(&self, position: IVec3) -> Option<BlockState> {
        const START_Y: i32 = -64;

        if position.y < START_Y {
            // This block is in the void.
            return Some(BlockState::VOID_AIR);
        }

        let chunk_pos: IVec2 = IVec2::new(position.x, position.z) >> 4;
        let chunk_start_block: IVec2 = chunk_pos << 4;

        let chunk = self.get_loaded_chunk(chunk_pos)?;

        let chunk = &chunk.data;
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
        position: IVec3,
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

        let Some((chunk_idx, _, chunk)) = self.chunk_cache.get_full_mut(&chunk_pos) else {
            return Err(TrySetBlockDeltaError::ChunkNotLoaded);
        };

        let x = u32::try_from(position.x - chunk_start_block[0]).unwrap();
        let y = u32::try_from(position.y - START_Y).unwrap();
        let z = u32::try_from(position.z - chunk_start_block[1]).unwrap();

        let old_state = chunk.data.set_delta(x, y, z, state);

        if old_state != state {
            self.should_update.insert(u32::try_from(chunk_idx).unwrap());
        }

        Ok(old_state)
    }

    // todo: allow modifying the chunk. we will need to implement resending
    // So,
    // for instance, if a player modifies a chunk, we're going to need to rebroadcast it to all the players in that region.
    // However, I'm going to wait until my broadcasting code using the new proxy is done before I do this.
    // If you want to implement this, I also recommend waiting until that's done.
    // That should be done in a couple of days, probably.

    #[must_use]
    pub fn get_cached(&self, position: IVec2) -> Option<Bytes> {
        if let Some(result) = self.chunk_cache.get(&position) {
            return Some(result.bytes());
        }

        None
    }

    /// get the cached chunk for the given position or load it if it is not cached.
    #[must_use]
    pub fn get_cached_or_load(&self, position: IVec2) -> GetChunk<'_> {
        if let Some(result) = self.chunk_cache.get(&position) {
            return GetChunk::Loaded(result);
        };

        self.loader_handle
            .send(position, self.tx_loaded_chunks.clone());

        GetChunk::Loading
    }
}
