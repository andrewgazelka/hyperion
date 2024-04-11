//! Defined the [`Global`] struct which is used to store global data which defines a [`crate::Game`]
use std::sync::{atomic::AtomicU32, Arc};

use evenio::component::Component;
use valence_protocol::CompressionThreshold;

/// Shared data that is shared between the ECS framework and the IO thread.
pub struct Shared {
    /// realistically, we will never have more than 2^32 = 4,294,967,296 players
    pub player_count: AtomicU32,
    /// The compression level to use for the server.
    pub compression_level: CompressionThreshold,
}

/// See [`crate::global`].
#[derive(Component)]
pub struct Global {
    /// The current tick of the game. This is incremented every 50 ms.
    pub tick: i64,
    /// Data shared between the IO thread and the ECS framework.
    pub shared: Arc<Shared>,
}
