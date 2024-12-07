use clap::Parser;
use flecs_ecs::core::{Entity, EntityView, EntityViewGet, WorldProvider};
use hyperion::{
    glam::Vec3,
    simulation::{Pitch, Position, Spawn, Uuid, Velocity, Yaw, entity_kind::EntityKind},
};
use hyperion_clap::{CommandPermission, MinecraftCommand};
use tracing::debug;

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "shoot")]
#[command_permission(group = "Normal")]
pub struct ShootCommand {
    #[arg(help = "Initial velocity of the arrow")]
    velocity: f32,
}

impl MinecraftCommand for ShootCommand {
    fn execute(self, system: EntityView<'_>, caller: Entity) {
        const EYE_HEIGHT: f32 = 1.62;
        const BASE_VELOCITY: f32 = 3.0; // Base velocity multiplier for arrows

        let world = system.world();

        caller
            .entity_view(world)
            .get::<(&Position, &Yaw, &Pitch)>(|(pos, yaw, pitch)| {
                // Calculate direction vector from player's rotation
                let direction = super::raycast::get_direction_from_rotation(**yaw, **pitch);

                // Spawn arrow slightly in front of player to avoid self-collision
                let spawn_pos = Vec3::new(pos.x, pos.y + EYE_HEIGHT, pos.z) + direction * 0.5;

                // Calculate velocity with base multiplier
                let velocity = direction * (self.velocity * BASE_VELOCITY);

                debug!(
                    "Arrow velocity: ({}, {}, {})",
                    velocity.x, velocity.y, velocity.z
                );

                // Create arrow entity with velocity
                world
                    .entity()
                    .add_enum(EntityKind::Arrow)
                    .set(Uuid::new_v4())
                    .set(Position::new(spawn_pos.x, spawn_pos.y, spawn_pos.z))
                    .set(Velocity::new(velocity.x, velocity.y, velocity.z))
                    .set(Yaw::new(**yaw))
                    .set(Pitch::new(**pitch))
                    .enqueue(Spawn);
            });
    }
}
