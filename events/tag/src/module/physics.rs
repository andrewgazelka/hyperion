use std::fmt::Debug;

use flecs_ecs::{
    core::{Entity, EntityView, EntityViewGet, World, WorldGet},
    prelude::*,
};
use geometry::ray::Ray;
use glam::Vec3;
use hyperion::{
    Prev,
    net::{Compose, ConnectionId, DataBundle},
    simulation::{
        EntitySize, Pitch, Position, Velocity, Yaw,
        animation::ActiveAnimation,
        blocks::{self, Blocks},
        entity_kind::EntityKind,
        handlers::try_change_position,
        metadata::entity::Pose,
    },
    valence_protocol::{ByteAngle, VarInt, packets::play},
};
use hyperion_utils::EntityExt;
use tracing::debug;

// Hitbox component to represent the bounding box of an entity in 3D
#[derive(Debug, Clone, Component)]
pub struct Hitbox {
    pub width: f32,  // Width of the hitbox
    pub height: f32, // Height of the hitbox
    pub depth: f32,  // Depth of the hitbox
}

impl Hitbox {
    // Constructor for Hitbox
    #[must_use]
    pub fn new(width: f32, height: f32, depth: f32) -> Self {
        Hitbox {
            width,
            height,
            depth,
        }
    }
}

#[derive(Component)]
pub struct PhysicsModule;

impl Module for PhysicsModule {
    fn module(world: &World) {
        // Register the Hitbox component
        world.component::<Hitbox>();

        system!(
            "update_physics",
            world,
            &Compose($),
            &mut Velocity,
            &mut Position,
            &mut (Prev, Position),
        )
        .multi_threaded()
        .kind::<flecs::pipeline::PreStore>()
        .each_iter(|it, row, (compose,velocity, position, prev_position)| {
            let world = it.system().world();

            // Calculate new position with current velocity
            let new_pos = Vec3::new(
                position.x + velocity.0.x,
                position.y + velocity.0.y,
                position.z + velocity.0.z,
            );

            let entity_size = EntitySize::default();

            // Try to move to new position, handling collisions
            world.get::<&mut Blocks>(|blocks| {
                match try_change_position(new_pos, position, entity_size, blocks) {
                    Ok(()) => {},
                    Err(err) => {
                        velocity.0.y = 0.0;
                    }
                }
            });

            // Apply drag
            velocity.0 *= 0.99;

            // Apply gravity if not on ground
            if prev_position.y != position.y {
                velocity.0.y -= 0.08;
            } else {
                velocity.0.y = 0.0;
            }

            if velocity.0.x < 0.4 {
                velocity.0.x = 0.0;
            }

            if velocity.0.z < 0.4 {
                velocity.0.z = 0.0;
            }
        });

        system!(
            "sync_entity_state",
            world,
            &Compose($),
            &mut (Prev, Position),
            &mut Position,
            &mut Velocity,
            &Yaw,
            &Pitch,
            ?&mut ActiveAnimation,
        )
        .multi_threaded()
        .kind::<flecs::pipeline::OnStore>()
        .each_iter(
            |it, row, (compose, prev_position, position, velocity, yaw, pitch, animation)| {
                let _world = it.system().world();
                let system = it.system();
                let entity = it.entity(row);
                let entity_id = VarInt(entity.minecraft_id());

                let chunk_pos = position.to_chunk();

                /* if velocity.0 != Vec3::ZERO {
                    /* let vel_packet = play::EntityVelocityUpdateS2c {
                        entity_id,
                        velocity: velocity.to_packet_units(),
                    }; */

                    // let new_pos = Vec3::new(position.x, position.y + velocity.0.y, position.z);
                    // let entity_size = EntitySize::default();
                    //
                    // world
                    // .get::<&mut Blocks>(|blocks| {
                    // debug!("Blocks");
                    // try_change_position(new_pos, position, entity_size, blocks).unwrap();
                    // });

                    /* compose
                        .broadcast_local(&vel_packet, chunk_pos, system)
                        .send()
                        .unwrap(); */
                } */

                // Send position and rotation
                let pos_packet = play::EntityPositionS2c {
                    entity_id,
                    position: position.as_dvec3(),
                    yaw: ByteAngle::from_degrees(**yaw),
                    pitch: ByteAngle::from_degrees(**pitch),
                    on_ground: velocity.0.y == 0.0,
                };

                compose
                    .broadcast_local(&pos_packet, chunk_pos, system)
                    .send()
                    .unwrap();

                let head_packet = play::EntitySetHeadYawS2c {
                    entity_id,
                    head_yaw: ByteAngle::from_degrees(**yaw),
                };

                compose
                    .broadcast_local(&head_packet, chunk_pos, system)
                    .send()
                    .unwrap();

                if let Some(animation) = animation {
                    for anim_packet in animation.packets(entity_id) {
                        compose
                            .broadcast_local(&anim_packet, chunk_pos, system)
                            .send()
                            .unwrap();
                    }
                    animation.clear();
                }

                // Send entity Pose
            },
        );
    }
}
