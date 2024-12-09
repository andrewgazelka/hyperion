use std::{
    fmt::Display,
    time::{Duration, SystemTime},
};

use flecs_ecs::prelude::*;
use humantime::format_duration;

#[derive(Component, Debug)]
pub struct BowCharging {
    pub start_time: SystemTime,
}

impl Display for BowCharging {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let duration = self.start_time.elapsed().unwrap_or_default();

        write!(f, "{}", format_duration(duration))
    }
}

impl BowCharging {
    #[must_use]
    pub fn now() -> Self {
        Self {
            start_time: SystemTime::now(),
        }
    }

    #[must_use]
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
