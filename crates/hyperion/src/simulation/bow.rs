use std::time::{Duration, SystemTime};
use flecs_ecs::prelude::*;

#[derive(Component, Debug)]
pub struct BowCharging {
    pub start_time: SystemTime,
}

impl BowCharging {
    pub fn new() -> Self {
        Self {
            start_time: SystemTime::now(),
        }
    }

    pub fn get_charge(&self) -> f32 {
        let elapsed = self.start_time.elapsed().unwrap_or(Duration::ZERO);
        let secs = elapsed.as_secs_f32();
        // Minecraft bow charge mechanics:
        // - Takes 1 second to fully charge
        // - Minimum charge is 0.0
        // - Maximum charge is 1.0
        secs.min(1.0)
    }
}
