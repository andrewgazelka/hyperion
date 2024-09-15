use std::fmt::Debug;

use bytes::Bytes;
use derive_more::{Deref, DerefMut};
use flecs_ecs::{
    core::{EntityViewGet, World},
    macros::Component,
};
use glam::I16Vec2;
use tracing::trace;
use valence_generated::block::BlockState;
use valence_protocol::{packets::play, BlockPos};
use valence_server::layer::chunk::Chunk;

use super::{loader::parse::UnloadedChunkWithMetadata, Block, MinecraftWorld};
use crate::{global::SystemId, net::Compose, storage::ThreadLocalVec};

pub const START_Y: i32 = -64;

// 384 / 16 = 24
// const CHUNK_HEIGHT: usize = 24;

// todo: bench packed vs non-packed cause we can pack xy into u8
// to get size_of::<Delta>() == 5
#[derive(Copy, Clone, Debug)]
pub struct Delta {
    x: u8,                   // 1
    z: u8,                   // 1
    y: u16,                  // 2
    block_state: BlockState, // 2
}

impl Delta {
    #[must_use]
    pub fn new(x: u8, y: u16, z: u8, block_state: BlockState) -> Self {
        debug_assert!(x <= 15);
        debug_assert!(z <= 15);
        debug_assert!(y <= 384);

        Self {
            x,
            z,
            y,
            block_state,
        }
    }
}

const _: () = assert!(size_of::<Delta>() == 6);

#[repr(packed)]
#[derive(Copy, Clone)]
pub struct OnChange {
    xz: u8, // 1
    y: u16, // 2
}

impl Debug for OnChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnChange")
            .field("x", &self.x())
            .field("z", &self.z())
            .field("y", &self.y())
            .finish()
    }
}

impl OnChange {
    #[must_use]
    pub const fn new(x: u8, y: u16, z: u8) -> Self {
        Self {
            xz: x << 4 | (z & 0b1111),
            y,
        }
    }

    #[must_use]
    pub const fn x(&self) -> u8 {
        self.xz >> 4
    }

    #[must_use]
    pub const fn z(&self) -> u8 {
        self.xz & 0b1111
    }

    #[must_use]
    pub const fn y(&self) -> u16 {
        self.y
    }
}

const _: () = assert!(size_of::<OnChange>() == 3);

#[derive(Debug, Default, Deref, DerefMut, Component)]
pub struct PendingChanges(ThreadLocalVec<Delta>);

#[derive(Debug, Default, Deref, DerefMut, Component)]
pub struct NeighborNotify(ThreadLocalVec<OnChange>);

/// A chunk which has been loaded into memory.
#[derive(Debug, Component)]
pub struct LoadedChunk {
    /// The raw (usually compressed) bytes of the chunk that are sent to the client via the Minecraft protocol.
    pub base_packet_bytes: Bytes,

    /// The actual chunk data that is "uncompressed". It uses a palette to store the actual data. This is usually used
    /// for obtaining the actual data from the chunk such as getting the block state of a block at a given position.
    pub chunk: UnloadedChunkWithMetadata,

    pub position: I16Vec2,
}

impl LoadedChunk {
    pub const fn new(
        base_packet_bytes: Bytes,
        chunk: UnloadedChunkWithMetadata,
        position: I16Vec2,
    ) -> Self {
        Self {
            base_packet_bytes,
            chunk,
            position,
        }
    }

    pub fn bytes(&self) -> Bytes {
        self.base_packet_bytes.clone()
    }

    #[must_use]
    pub const fn chunk(&self) -> &UnloadedChunkWithMetadata {
        &self.chunk
    }

    pub fn chunk_mut(&mut self) -> &mut UnloadedChunkWithMetadata {
        &mut self.chunk
    }

    fn set_block_internal(&mut self, x: u8, y: u16, z: u8, state: BlockState) {
        self.chunk
            .set_block(u32::from(x), u32::from(y), u32::from(z), state);
    }

    fn get_block_internal(&self, x: u8, y: u16, z: u8) -> u16 {
        self.chunk
            .block_state(u32::from(x), u32::from(y), u32::from(z))
            .to_raw()
    }

    fn get_block(&self, x: u8, y: u16, z: u8) -> BlockState {
        BlockState::from_raw(self.get_block_internal(x, y, z)).unwrap()
    }

    pub fn process_neighbor_changes(
        &self,
        pending: &mut NeighborNotify,
        mc: &MinecraftWorld,
        world: &World,
    ) {
        let position = self.position;

        let start_x = i32::from(position.x) << 4;
        let start_z = i32::from(position.y) << 4;

        for change in pending.drain() {
            let x = change.x();
            let z = change.z();
            let y = change.y();

            let state = self.get_block(x, y, z);

            let x = i32::from(x) + start_x;
            let z = i32::from(z) + start_z;
            let y = i32::from(y) + START_Y;

            let block_pos = BlockPos::new(x, y, z);

            let block = Block::from(state);

            (block.on_neighbor_block_change)(mc, block_pos, state, world);
        }
    }

    pub fn interact(&self, x: u8, y: u16, z: u8, mc: &MinecraftWorld, world: &World) {
        let position = self.position;

        let state = self.get_block(x, y, z);

        let start_x = i32::from(position.x) << 4;
        let start_z = i32::from(position.y) << 4;

        let x = i32::from(x) + start_x;
        let z = i32::from(z) + start_z;
        let y = i32::from(y) + START_Y;

        let block_pos = BlockPos::new(x, y, z);

        let block = Block::from(state);

        (block.on_block_interact)(mc, block_pos, state, world);
    }

    pub fn process_pending_changes(
        &mut self,
        current_deltas: &mut PendingChanges,
        compose: &Compose,
        notify: &NeighborNotify,
        mc: &MinecraftWorld,
        system_id: SystemId,
        world: &World,
    ) {
        const MAX_Y: u16 = 384;

        let position = self.position;

        for Delta {
            x,
            y,
            z,
            block_state,
        } in current_deltas.drain()
        {
            self.set_block_internal(x, y, z, block_state);

            trace!("set block at {x} {y} {z} to {block_state}");

            let start_x = i32::from(position.x) << 4;
            let start_z = i32::from(position.y) << 4;

            let block_pos = BlockPos::new(
                start_x + i32::from(x),
                START_Y + i32::from(y),
                start_z + i32::from(z),
            );

            let pkt = play::BlockUpdateS2c {
                position: block_pos,
                block_id: block_state,
            };

            compose.broadcast(&pkt, system_id).send(world).unwrap();

            // notify neighbors
            if x == 0 {
                let chunk_position = position - I16Vec2::new(1, 0);
                if let Some(entity) = mc.get_loaded_chunk_entity(chunk_position, world) {
                    entity.get::<&NeighborNotify>(|notify| {
                        notify.push(OnChange::new(15, y, z), world);
                    });
                };
            } else {
                notify.push(OnChange::new(x - 1, y, z), world);
            }

            if x == 15 {
                let chunk_position = position + I16Vec2::new(1, 0);
                if let Some(entity) = mc.get_loaded_chunk_entity(chunk_position, world) {
                    entity.get::<&NeighborNotify>(|notify| {
                        notify.push(OnChange::new(0, y, z), world);
                    });
                };
            } else {
                notify.push(OnChange::new(x + 1, y, z), world);
            }

            if y != 0 {
                notify.push(OnChange::new(x, y - 1, z), world);
            }

            if y != MAX_Y {
                // todo: is this one off?
                notify.push(OnChange::new(x, y + 1, z), world);
            }

            if z == 0 {
                let chunk_position = position - I16Vec2::new(0, 1);
                if let Some(entity) = mc.get_loaded_chunk_entity(chunk_position, world) {
                    entity.get::<&NeighborNotify>(|notify| {
                        notify.push(OnChange::new(x, y, 15), world);
                    });
                };
            } else {
                notify.push(OnChange::new(x, y, z - 1), world);
            }

            if z == 15 {
                let chunk_position = position + I16Vec2::new(0, 1);
                if let Some(entity) = mc.get_loaded_chunk_entity(chunk_position, world) {
                    entity.get::<&NeighborNotify>(|notify| {
                        notify.push(OnChange::new(x, y, 0), world);
                    });
                };
            } else {
                notify.push(OnChange::new(x, y, z + 1), world);
            }
        }
    }
}
