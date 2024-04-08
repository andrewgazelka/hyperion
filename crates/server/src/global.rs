use std::sync::{atomic::AtomicU32, Arc};

use evenio::component::Component;
use valence_protocol::CompressionThreshold;

pub struct Shared {
    // realistically, we will never have more than 2^32 = 4,294,967,296 players
    pub player_count: AtomicU32,
    pub compression_level: CompressionThreshold,
}

#[derive(Component)]
pub struct Global {
    pub tick: i64,
    pub shared: Arc<Shared>,
}
