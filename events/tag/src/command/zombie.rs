use clap::Parser;
use flecs_ecs::core::{Entity, World};
use hyperion::simulation::{Pitch, Position, Spawn, Uuid, Velocity, Yaw, entity_kind::EntityKind};
use hyperion_clap::{CommandPermission, MinecraftCommand};

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "spawn")]
#[command_permission(group = "Normal")]
pub struct SpawnCommand;

impl MinecraftCommand for SpawnCommand {
    fn execute(self, world: &World, _caller: Entity) {
        world
            .entity()
            .add_enum(EntityKind::BlockDisplay)
            // .set(Uuid::new_v4())
            // .add_enum(EntityKind::Zombie)
            .set(Position::new(0.0, 20.0, 0.0))
            .set(Pitch::new(0.0))
            .set(Yaw::new(0.0))
            .set(Velocity::ZERO)
            .enqueue(Spawn);
    }
}
