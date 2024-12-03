use clap::Parser;
use flecs_ecs::core::{Entity, World};
use hyperion::simulation::{EntityKind, Position, Spawn};
use hyperion_clap::{CommandPermission, MinecraftCommand};

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "zombie")]
#[command_permission(group = "Normal")]
pub struct ZombieCommand;

impl MinecraftCommand for ZombieCommand {
    fn execute(self, world: &World, _caller: Entity) {
        world
            .entity()
            .set(EntityKind::ZOMBIE)
            .set(Position::new(0.0, 20.0, 0.0))
            // .set(Pitch::new(0.0))
            // .set(Yaw::new(0.0))
            // .set(Velocity::ZERO)
            .enqueue(Spawn);
    }
}
