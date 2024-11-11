use flecs_ecs::core::World;
use hyperion_clap::{MinecraftCommand, hyperion_command::CommandRegistry};

use crate::command::{fly::FlyCommand, speed::SpeedCommand};

mod fly;
mod speed;

pub fn register(registry: &mut CommandRegistry, world: &World) {
    SpeedCommand::register(registry, world);
    FlyCommand::register(registry, world);
}
