include!(concat!(env!("OUT_DIR"), "/shared.rs"));

impl ChunkPosition {
    #[must_use]
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}
