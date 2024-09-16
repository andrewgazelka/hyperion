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

use super::{loader::parse::UnloadedChunkWithMetadata, MinecraftWorld};
use crate::{net::Compose, storage::ThreadLocalVec, system_registry::SystemId};

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

mod packet;

/// A chunk which has been loaded into memory.
#[derive(Debug)]
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
}
