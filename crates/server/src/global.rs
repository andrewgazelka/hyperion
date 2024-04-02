use std::sync::atomic::AtomicU32;

use evenio::component::Component;

pub struct Shared {
    // realistically, we will never have more than 2^32 = 4,294,967,296 players
    pub player_count: AtomicU32,
}

#[derive(Component)]
pub struct Global {
    pub world_border_diameter: Option<f64>,
    pub tick: i64,
}

impl Default for Global {
    fn default() -> Self {
        Self {
            world_border_diameter: Some(100.0),
            tick: 0,
        }
    }
}
