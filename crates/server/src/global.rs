//! Defined the [`Global`] struct which is used to store global data which defines a [`crate::Game`]
use std::{
    cell::Cell,
    sync::{atomic::AtomicU32, Arc},
    time::Duration,
};

use evenio::component::Component;
use rayon_local::RayonLocal;
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

    /// The maximum amount of time a player is resistant to being hurt. This is weird as this is 20 in vanilla
    /// Minecraft.
    /// However, the check to determine if a player can be hurt actually looks at this value divided by 2
    pub max_hurt_resistant_time: u16,

    /// Data shared between the IO thread and the ECS framework.
    pub shared: Arc<Shared>,

    needs_realloc: RayonLocal<Cell<bool>>,

    pub keep_alive_timeout: Duration,
}

impl Global {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            tick: 0,
            max_hurt_resistant_time: 20, // actually kinda like 10 vanilla mc is weird
            shared,
            needs_realloc: RayonLocal::init_with(|| Cell::new(false)),
            keep_alive_timeout: Duration::from_secs(20),
        }
    }

    pub fn set_needs_realloc(&self) {
        self.needs_realloc.get_rayon_local().set(true);
    }

    pub fn get_needs_realloc(&mut self) -> bool {
        self.needs_realloc.get_all_locals().iter().any(Cell::get)
    }
}
