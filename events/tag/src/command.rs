use flecs_ecs::core::World;
use hyperion_clap::{MinecraftCommand, hyperion_command::CommandRegistry};

use crate::command::{
    class::ClassCommand, fly::FlyCommand, gui::GuiCommand, raycast::RaycastCommand,
    replace::ReplaceCommand, shoot::ShootCommand, spawn::SpawnCommand, speed::SpeedCommand,
    xp::XpCommand,
};

mod class;
mod fly;
mod gui;
mod raycast;
mod replace;
mod shoot;
mod spawn;
mod speed;
mod xp;

pub fn register(registry: &mut CommandRegistry, world: &World) {
    ClassCommand::register(registry, world);
    FlyCommand::register(registry, world);
    RaycastCommand::register(registry, world);
    ReplaceCommand::register(registry, world);
    ShootCommand::register(registry, world);
    SpeedCommand::register(registry, world);
    XpCommand::register(registry, world);
    SpawnCommand::register(registry, world);
    GuiCommand::register(registry, world);
}
