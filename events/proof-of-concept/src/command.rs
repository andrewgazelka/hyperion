use flecs_ecs::core::World;
use hyperion_clap::{MinecraftCommand, hyperion_command::CommandRegistry};

use crate::command::{fly::FlyCommand, rank::RankCommand, speed::SpeedCommand};

mod fly;
mod rank;
mod speed;

pub fn register(registry: &mut CommandRegistry, world: &World) {
    SpeedCommand::register(registry, world);
    FlyCommand::register(registry, world);
    RankCommand::register(registry, world);
}
