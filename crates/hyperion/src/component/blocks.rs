//! Constructs for working with blocks.

use std::{collections::HashMap, ops::Try, sync::Arc};

use bytes::Bytes;
use compact_str::format_compact;
use flecs_ecs::{
    core::{Entity, EntityView, EntityViewGet, IdOperations, World},
    macros::Component,
};
use fxhash::FxHashMap;
use glam::{I16Vec2, IVec2};
use tracing::{info, instrument};
use valence_generated::block::{BlockKind, BlockState, PropName, PropValue};
use valence_protocol::{BlockPos, Direction};
use valence_registry::BiomeRegistry;
use valence_server::layer::chunk::Chunk;

use crate::{
    component::blocks::{
        chunk::{Delta, LoadedChunk, NeighborNotify, PendingChanges},
        loader::{launch_manager, LaunchHandle},
        shared::Shared,
    },
    event::EventQueue,
    runtime::AsyncRuntime,
};

pub mod chunk;
pub mod interact;

mod loader;
mod manager;

mod region;

mod shared;

pub enum GetChunkBytes {
    Loaded(Bytes),
    Loading,
}

/// Accessor of blocks.
#[derive(Component)]
pub struct MinecraftWorld {
    /// Map to a Chunk by Entity ID
    chunk_cache: FxHashMap<I16Vec2, Entity>,

    launch_manager: LaunchHandle,

    tx_loaded_chunks: tokio::sync::mpsc::UnboundedSender<LoadedChunk>,
    rx_loaded_chunks: tokio::sync::mpsc::UnboundedReceiver<LoadedChunk>,
}

impl MinecraftWorld {
    pub(crate) fn new(registry: &BiomeRegistry, runtime: AsyncRuntime) -> anyhow::Result<Self> {
        let shared = Shared::new(registry, &runtime)?;
        let shared = Arc::new(shared);

        let (tx_loaded_chunks, rx_loaded_chunks) = tokio::sync::mpsc::unbounded_channel();

        Ok(Self {
            chunk_cache: HashMap::default(),
            launch_manager: launch_manager(shared, runtime),
            tx_loaded_chunks,
            rx_loaded_chunks,
        })
    }

    /// get_and_wait can only be called if a chunk has not already been loaded
    #[allow(clippy::missing_panics_doc, reason = "todo use unwrap unchecked")]
    #[instrument(skip_all)]
    pub unsafe fn get_and_wait(
        &self,
        position: I16Vec2,
        tasks: &AsyncRuntime,
        world: &World,
    ) -> Bytes {
        if let Some(cached) = self.get_cached(position, world) {
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

    pub fn load_pending(&mut self, world: &World) {
        while let Ok(chunk) = self.rx_loaded_chunks.try_recv() {
            let position = chunk.position;
            
            let x = position[0];
            let z = position[1];

            let name = format_compact!("chunk_{x:03}_{z:03}");

            let entity = world
                .entity_named(&name)
                .set(chunk)
                .set(EventQueue::default())
                .set(NeighborNotify::default())
                .set(PendingChanges::default());

            self.chunk_cache.insert(position, entity.id());
        }
    }

    #[must_use]
    pub fn get_loaded_chunk_entity<'a>(
        &self,
        chunk_position: I16Vec2,
        world: &'a World,
    ) -> Option<EntityView<'a>> {
        let entity = self.chunk_cache.get(&chunk_position).copied()?;
        Some(world.entity_from_id(entity))
    }

    /// Returns the unloaded chunk if it is loaded, otherwise `None`.
    // todo: return type: what do you think about the type right here?
    // This seems really complicated.
    // I wonder if we can just implement something, where we can return an `impl Deref`
    // and see if this would make more sense or not.
    #[must_use]
    pub fn get_loaded_chunk<R>(
        &self,
        chunk_position: I16Vec2,
        world: &World,
        f: impl FnOnce(Option<&LoadedChunk>) -> R,
    ) -> R {
        let Some(entity) = self.get_loaded_chunk_entity(chunk_position, world) else {
            return f(None);
        };

        let entity = world.entity_from_id(entity);

        entity.get::<&LoadedChunk>(|chunk| f(Some(chunk)))
    }

    /// Returns all loaded blocks within the range from `start` to `end` (inclusive).
    #[allow(clippy::excessive_nesting)]
    pub fn get_blocks<F, R>(&self, start: BlockPos, end: BlockPos, world: &World, mut f: F) -> R
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

                self.get_loaded_chunk(chunk_pos, world, |chunk| {
                    let Some(chunk) = chunk else {
                        return R::from_output(());
                    };

                    let chunk = &chunk.chunk;
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

                    // todo: pretty sure something is wrong here
                    R::from_output(())
                })?;
            }
        }

        R::from_output(())
    }

    /// Get a block
    #[must_use]
    pub fn get_block(&self, position: BlockPos, world: &World) -> Option<BlockState> {
        const START_Y: i32 = -64;

        if position.y < START_Y {
            // This block is in the void.
            return Some(BlockState::VOID_AIR);
        }

        let chunk_pos: IVec2 = IVec2::new(position.x, position.z) >> 4;
        let chunk_start_block: IVec2 = chunk_pos << 4;
        let chunk_pos = chunk_pos.as_i16vec2();

        self.get_loaded_chunk(chunk_pos, world, |chunk| {
            let chunk = chunk?;

            let chunk = &chunk.chunk;
            // todo: is this right for negative numbers?
            // I have no idea... let's test
            // non-absolute difference should work as well, but we want a u32
            let x = u32::try_from(position.x - chunk_start_block[0]).unwrap();
            let y = u32::try_from(position.y - START_Y).unwrap();
            let z = u32::try_from(position.z - chunk_start_block[1]).unwrap();

            Some(chunk.block_state(x, y, z))
        })
    }

    pub fn set_block(&self, position: BlockPos, state: BlockState, world: &World) {
        const START_Y: i32 = -64;

        if position.y < START_Y {
            // This block is in the void.
            return;
        }

        let chunk_pos: IVec2 = IVec2::new(position.x, position.z) >> 4;
        let chunk_start_block: IVec2 = chunk_pos << 4;
        let chunk_pos = chunk_pos.as_i16vec2();

        let chunk = self.get_loaded_chunk_entity(chunk_pos, world).unwrap();

        chunk.get::<&PendingChanges>(|changes| {
            // non-absolute difference should work as well, but we want a u32
            let x = u8::try_from(position.x - chunk_start_block[0]).unwrap();
            let y = u16::try_from(position.y - START_Y).unwrap();
            let z = u8::try_from(position.z - chunk_start_block[1]).unwrap();

            let delta = Delta::new(x, y, z, state);

            changes.push(delta, world);
        });
    }

    // todo: allow modifying the chunk. we will need to implement resending
    // So,
    // for instance, if a player modifies a chunk, we're going to need to rebroadcast it to all the players in that region.
    // However, I'm going to wait until my broadcasting code using the new proxy is done before I do this.
    // If you want to implement this, I also recommend waiting until that's done.
    // That should be done in a couple of days, probably.

    pub fn get_cached(&self, position: I16Vec2, world: &World) -> Option<Bytes> {
        if let Some(&result) = self.chunk_cache.get(&position) {
            let result = world.entity_from_id(result);
            let result = result.get::<&LoadedChunk>(LoadedChunk::bytes);

            return Some(result);
        }

        None
    }

    /// get the cached chunk for the given position or load it if it is not cached.
    #[must_use]
    pub fn get_cached_or_load(&self, position: I16Vec2, world: &World) -> GetChunkBytes {
        if let Some(result) = self.get_cached(position, world) {
            return GetChunkBytes::Loaded(result);
        };

        self.launch_manager
            .send(position, self.tx_loaded_chunks.clone());

        GetChunkBytes::Loading
    }
}

struct Block {
    on_block_interact:
        fn(blocks: &MinecraftWorld, block_pos: BlockPos, state: BlockState, world: &World),
    on_neighbor_block_change:
        fn(blocks: &MinecraftWorld, block_pos: BlockPos, state: BlockState, world: &World),
}

impl Default for Block {
    fn default() -> Self {
        Self {
            on_block_interact: |_, _, _, _| {},
            on_neighbor_block_change: |_, _, _, _| {},
        }
    }
}

impl From<BlockState> for Block {
    fn from(state: BlockState) -> Self {
        match state.to_kind() {
            BlockKind::OakDoor => DOOR,
            _ => Self::default(),
        }
    }
}

trait BlockInfo {
    fn on_block_interact(
        blocks: &MinecraftWorld,
        block_pos: BlockPos,
        state: BlockState,
        world: &World,
    );
    fn on_neighbor_block_change(
        blocks: &MinecraftWorld,
        block_pos: BlockPos,
        state: BlockState,
        world: &World,
    );
}

struct Door;

impl BlockInfo for Door {
    fn on_block_interact(
        blocks: &MinecraftWorld,
        block_pos: BlockPos,
        state: BlockState,
        world: &World,
    ) {
        let value = state.get(PropName::Open).unwrap();

        // toggle
        let open_prop = match value {
            PropValue::True => PropValue::False,
            PropValue::False => PropValue::True,
            _ => unreachable!(),
        };

        blocks.set_block(block_pos, state.set(PropName::Open, open_prop), world);

        match state.get(PropName::Half).unwrap() {
            PropValue::Upper => {
                let below = block_pos.get_in_direction(Direction::Down);
                let Some(below_state) = blocks.get_block(below, world) else {
                    return;
                };

                blocks.set_block(below, below_state.set(PropName::Open, open_prop), world);
            }
            PropValue::Lower => {
                let above = block_pos.get_in_direction(Direction::Up);
                let Some(above_state) = blocks.get_block(above, world) else {
                    return;
                };

                blocks.set_block(above, above_state.set(PropName::Open, open_prop), world);
            }
            _ => unreachable!(),
        }
    }

    #[instrument(skip_all)]
    fn on_neighbor_block_change(
        blocks: &MinecraftWorld,
        block_pos: BlockPos,
        state: BlockState,
        world: &World,
    ) {
        let value = state.get(PropName::Half).unwrap();

        let open = state.get(PropName::Open).unwrap();

        match value {
            PropValue::Upper => {
                let Some(below) =
                    blocks.get_block(block_pos.get_in_direction(Direction::Down), world)
                else {
                    return;
                };

                if below.to_kind() == BlockKind::OakDoor {
                    let below_open = below.get(PropName::Open).unwrap();

                    if below_open != open {
                        let state = state.set(PropName::Open, below_open);
                        info!("adj to be same as below");
                        blocks.set_block(block_pos, state, world);
                    }

                    return;
                }

                info!("set air at {block_pos}");
                blocks.set_block(block_pos, BlockState::AIR, world);
            }
            PropValue::Lower => {
                let Some(above) =
                    blocks.get_block(block_pos.get_in_direction(Direction::Up), world)
                else {
                    return;
                };

                if above.to_kind() == BlockKind::OakDoor {
                    let above_open = above.get(PropName::Open).unwrap();

                    if above_open != open {
                        let state = state.set(PropName::Open, above_open);
                        info!("adj to be same as above");
                        blocks.set_block(block_pos, state, world);
                    }

                    return;
                }

                info!("set air at {block_pos}");
                blocks.set_block(block_pos, BlockState::AIR, world);
            }
            _ => unreachable!(),
        }
    }
}

const DOOR: Block = Block {
    on_block_interact: Door::on_block_interact,
    on_neighbor_block_change: Door::on_neighbor_block_change,
};
