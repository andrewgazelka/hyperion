use clap::Parser;
use flecs_ecs::core::{Entity, World, WorldGet};
use hyperion::{
    BlockState,
    simulation::{
        Pitch, Position, Spawn, Uuid, Velocity, Yaw,
        entity_kind::EntityKind,
        metadata::{
            MetadataPrefabs,
            block_display::DisplayedBlockState,
            display::{Height, ViewRange, Width},
        },
    },
};
use hyperion_clap::{CommandPermission, MinecraftCommand};

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "spawn")]
#[command_permission(group = "Normal")]
pub struct SpawnCommand;

impl MinecraftCommand for SpawnCommand {
    fn execute(self, world: &World, _caller: Entity) {
        world.get::<&MetadataPrefabs>(|prefabs| {
            let block_display = prefabs.block_display_base;

            world
                .entity()
                .add_enum(EntityKind::BlockDisplay)
                .set(Uuid::new_v4())
                .set(Width::new(1.0))
                .set(Height::new(1.0))
                .set(ViewRange::new(100.0))
                // .add_enum(EntityKind::Zombie)
                .set(Position::new(0.0, 22.0, 0.0))
                .set(Pitch::new(0.0))
                .set(Yaw::new(0.0))
                .set(Velocity::ZERO)
                .set(DisplayedBlockState::new(BlockState::DIRT))
                .is_a_id(block_display)
                .enqueue(Spawn);
        });
    }
}
