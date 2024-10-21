use rkyv::{Archive, Deserialize, Serialize};

#[derive(Archive, Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
#[rkyv(derive(Debug))]
pub struct ChunkPosition {
    pub x: i16,
    pub z: i16,
}

impl ChunkPosition {
    #[must_use]
    pub const fn new(x: i16, z: i16) -> Self {
        Self { x, z }
    }
}
