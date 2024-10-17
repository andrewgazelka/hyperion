use std::fmt::Debug;

use bytes::Bytes;
use glam::I16Vec2;
use valence_generated::block::BlockState;
use valence_server::layer::chunk::Chunk;
use crate::simulation::blocks::loader::CHUNK_HEIGHT_SPAN;
use super::loader::parse::ChunkData;

pub const START_Y: i32 = -64;

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

mod packet;

/// A chunk which has been loaded into memory.
#[derive(Debug)]
pub struct LoadedChunk {
    /// The raw (usually compressed) bytes of the chunk that are sent to the client via the Minecraft protocol.
    pub base_packet_bytes: Bytes,

    /// The actual chunk data that is "uncompressed". It uses a palette to store the actual data. This is usually used
    /// for obtaining the actual data from the chunk such as getting the block state of a block at a given position.
    pub chunk: ChunkData,

    pub position: I16Vec2,
}

impl LoadedChunk {
    pub fn new(base_packet_bytes: Bytes, chunk: ChunkData, position: I16Vec2) -> Self {
        Self {
            base_packet_bytes,
            chunk,
            position,
        }
    }

    pub fn bytes(&self) -> Bytes {
        self.base_packet_bytes.clone()
    }

    #[expect(unused, reason = "might be useful in the future")]
    fn set_block_internal(&mut self, x: u8, y: u16, z: u8, state: BlockState) {
        self.chunk
            .set_block(u32::from(x), u32::from(y), u32::from(z), state);
    }

    fn get_block_internal(&self, x: u8, y: u16, z: u8) -> u16 {
        self.chunk
            .block_state(u32::from(x), u32::from(y), u32::from(z))
            .to_raw()
    }

    #[expect(unused, reason = "might be useful in the future")]
    fn get_block(&self, x: u8, y: u16, z: u8) -> BlockState {
        BlockState::from_raw(self.get_block_internal(x, y, z)).unwrap()
    }
}
