use std::fmt::Debug;

use flecs_ecs::{
    core::{Entity, EntityView, EntityViewGet, World, WorldGet},
    prelude::*,
};
use geometry::ray::Ray;
use glam::Vec3;
use hyperion::{
    net::{Compose, DataBundle},
    simulation::{
        animation::ActiveAnimation, blocks::Blocks, entity_kind::EntityKind, Pitch, Position, Velocity, Yaw
    },
    valence_protocol::{packets::play, ByteAngle, VarInt}, Prev,
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

        // Define the physics system using the system! macro
        system!("update_entity_state", world, &mut Velocity, &mut Position,)
            .multi_threaded()
            .with_enum_wildcard::<EntityKind>()
            .kind::<flecs::pipeline::PreStore>()
            .each_iter(|it, row, (velocity, position)| {
                let entity = it.entity(row);

                let world = it.system().world();

                entity.entity_view(world).try_get::<&EntityKind>(|kind| {
                    world.get::<&Blocks>(|blocks| {
                        // pain and agony
                        let hitbox = match kind {
                            EntityKind::Allay => [0.6, 0.35, 0.6],
                            EntityKind::ChestBoat | EntityKind::Boat => [1.375, 0.5625, 1.375],
                            EntityKind::Frog => [0.5, 0.5, 0.5],
                            EntityKind::Tadpole => [0.4, 0.3, 0.4],
                            EntityKind::SpectralArrow | EntityKind::Arrow => [0.5, 0.5, 0.5],
                            EntityKind::Axolotl => [1.3, 0.6, 1.3],
                            EntityKind::Bat => [0.5, 0.9, 0.5],
                            EntityKind::Blaze => [0.6, 1.8, 0.6],
                            EntityKind::Cat => [0.6, 0.7, 0.6],
                            EntityKind::CaveSpider => [0.7, 0.5, 0.7],
                            EntityKind::Cod => [0.5, 0.3, 0.5],
                            EntityKind::Creeper => [0.6, 1.7, 0.6],
                            EntityKind::Dolphin => [0.9, 0.6, 0.9],
                            EntityKind::DragonFireball => [1.0, 1.0, 1.0],
                            EntityKind::ElderGuardian => [1.9975, 1.9975, 1.9975],
                            EntityKind::EndCrystal => [2.0, 2.0, 2.0],
                            EntityKind::EnderDragon => [16.0, 8.0, 16.0],
                            EntityKind::Enderman => [0.6, 2.9, 0.6],
                            EntityKind::Endermite => [0.4, 0.3, 0.4],
                            EntityKind::Evoker => [0.6, 1.95, 0.6],
                            EntityKind::EvokerFangs => [0.5, 0.8, 0.5],
                            EntityKind::ExperienceOrb => [0.5, 0.5, 0.5],
                            EntityKind::EyeOfEnder => [0.25, 0.25, 0.25],
                            EntityKind::FallingBlock => [0.98, 0.98, 0.98],
                            EntityKind::FireworkRocket => [0.25, 0.25, 0.25],
                            EntityKind::Ghast => [4.0, 4.0, 4.0],
                            EntityKind::Giant => [3.6, 12.0, 3.6],
                            EntityKind::GlowSquid | EntityKind::Squid => [0.8, 0.8, 0.8],
                            EntityKind::Guardian => [0.85, 0.85, 0.85],
                            EntityKind::Illusioner => [0.6, 1.95, 0.6],
                            EntityKind::IronGolem => [1.4, 2.7, 1.4],
                            EntityKind::Item => [0.25, 0.25, 0.25],
                            EntityKind::Fireball => [1.0, 1.0, 1.0],
                            EntityKind::LeashKnot => [0.375, 0.5, 0.375],
                            EntityKind::Lightning /* | EntityKind::MARKER - marker hitbox */ => [0.0; 3],
                            EntityKind::LlamaSpit => [0.25, 0.25, 0.25],
                            EntityKind::Minecart
                            | EntityKind::ChestMinecart
                            | EntityKind::TntMinecart
                            | EntityKind::HopperMinecart
                            | EntityKind::FurnaceMinecart
                            | EntityKind::SpawnerMinecart
                            | EntityKind::CommandBlockMinecart => [0.98, 0.7, 0.98],
                            EntityKind::Parrot => [0.5, 0.9, 0.5],
                            EntityKind::Phantom => [0.9, 0.5, 0.9],
                            EntityKind::PiglinBrute => [0.6, 1.95, 0.6],
                            EntityKind::Pillager => [0.6, 1.95, 0.6],
                            EntityKind::Tnt => [0.98, 0.98, 0.98],
                            EntityKind::Pufferfish => [0.7, 0.7, 0.7],
                            EntityKind::Ravager => [1.95, 2.2, 1.95],
                            EntityKind::Salmon => [0.7, 0.4, 0.7],
                            EntityKind::ShulkerBullet => [0.3125, 0.3125, 0.3125],
                            EntityKind::Silverfish => [0.4, 0.3, 0.4],
                            EntityKind::SmallFireball => [0.3125, 0.3125, 0.3125],
                            EntityKind::SnowGolem => [0.7, 1.9, 0.7],
                            EntityKind::Spider => [1.4, 0.9, 1.4],
                            EntityKind::Stray => [0.6, 1.99, 0.6],
                            EntityKind::Egg => [0.25, 0.25, 0.25],
                            EntityKind::EnderPearl => [0.25, 0.25, 0.25],
                            EntityKind::ExperienceBottle => [0.25, 0.25, 0.25],
                            EntityKind::Player => [0.6, 1.8, 0.6],
                            EntityKind::Potion => [0.25, 0.25, 0.25],
                            EntityKind::Trident => [0.5, 0.5, 0.5],
                            EntityKind::TraderLlama => [0.9, 1.87, 0.9],
                            EntityKind::TropicalFish => [0.5, 0.4, 0.5],
                            EntityKind::Vex => [0.4, 0.8, 0.4],
                            EntityKind::Vindicator => [0.6, 1.95, 0.6],
                            EntityKind::Wither => [0.9, 3.5, 0.9],
                            EntityKind::WitherSkeleton => [0.7, 2.4, 0.7],
                            EntityKind::WitherSkull => [0.3125, 0.3125, 0.3125],
                            EntityKind::FishingBobber => [0.25, 0.25, 0.25],
                            EntityKind::Bee => [0.7, 0.6, 0.7],
                            EntityKind::Camel => [1.7, 2.375, 1.7],
                            EntityKind::Chicken => [0.4, 0.7, 0.4],
                            EntityKind::Donkey => [1.5, 1.39648, 1.5],
                            EntityKind::Fox => [0.6, 0.7, 0.6],
                            /* EntityKind::Goat => {
                                if pose_query
                                    .get(entity)
                                    .map_or(false, |v| v.0 == Pose::LongJumping)
                                {
                                    [0.63, 0.91, 0.63]
                                } else {
                                    [0.9, 1.3, 0.9]
                                }
                            } */
                            EntityKind::Hoglin => [1.39648, 1.4, 1.39648],
                            EntityKind::Horse | EntityKind::SkeletonHorse | EntityKind::ZombieHorse => {
                                [1.39648, 1.6, 1.39648]
                            }
                            EntityKind::Llama => [0.9, 1.87, 0.9],
                            EntityKind::Mule => [1.39648, 1.6, 1.39648],
                            EntityKind::Mooshroom => [0.9, 1.4, 0.9],
                            EntityKind::Ocelot => [0.6, 0.7, 0.6],
                            EntityKind::Panda => [1.3, 1.25, 1.3],
                            EntityKind::Pig => [0.9, 0.9, 0.9],
                            EntityKind::PolarBear => [1.4, 1.4, 1.4],
                            EntityKind::Rabbit => [0.4, 0.5, 0.4],
                            EntityKind::Sheep => [0.9, 1.3, 0.9],
                            /* EntityKind::TURTLE => {
                                hitbox.centered(
                                    if child.0 {
                                        [0.36, 0.12, 0.36]
                                    } else {
                                        [1.2, 0.4, 1.2]
                                    }
                                    .into(),
                                );
                            }, */
                            EntityKind::Villager => [0.6, 1.95, 0.6],
                            EntityKind::Wolf => [0.6, 0.85, 0.6],
                                    _ => [1.0, 1.0, 1.0]
                        };
                        let hitbox = Hitbox {
                            width: hitbox[0],
                            height: hitbox[1],
                            depth: hitbox[2],
                        };

                        let dt = it.delta_time();

                        velocity.0.y -= 0.08;
                        // debug!("Velocity: ({}, {}, {})", velocity.0.x, velocity.0.y, velocity.0.z);
                        if velocity.0.y > 3.92 {
                            velocity.0.y = 3.92;
                        }
                        // Apply drag
                        velocity.0.x *= 0.99;
                        velocity.0.y *= 0.99;
                        velocity.0.z *= 0.99;             
                                
                        let eye = Vec3::new(position.x, position.y, position.z);
                        let ray_distance = 2.0; // Increased ray distance to better detect ground
                        let direction = Vec3::new(0.0, -ray_distance, 0.0);
                        let ray = Ray::new(eye, direction);

                        // Store previous position for collision resolution
                        let prev_pos = *position;
                        
                        // Apply velocity
                        **position += velocity.0;

                        // Check for collision with blocks
                        if let Some(collision) = blocks.first_collision(ray, hitbox.height) {
                            // Calculate the ground height
                            let ground_height = collision.location.y as f32 + hitbox.height;
                            
                            // Only adjust if we're above the collision point
                            if position.y <= ground_height {
                                position.y = ground_height;
                                velocity.0.y = 0.0;
                                debug!("Grounded at y={}", ground_height);
                            }
                        }
                    });
                });
            });

        system!(
            "sync_entity_state",
            world,
            &Compose($),
            &mut (Prev, Position),
            &mut (Prev, Yaw),
            &mut (Prev, Pitch),
            &mut Position,
            &Velocity,
            &Yaw,
            &Pitch,
        )
        .multi_threaded()
        .kind::<flecs::pipeline::OnStore>()
        .each_iter(|it, row, (compose, prev_position, _prev_yaw, _prev_pitch, position, velocity, yaw, pitch)| {
            let world = it.system().world();
            let system = it.system();

            let entity = it.entity(row);

            let entity_id = VarInt(entity.minecraft_id()); // Assuming entity has a method to get its Minecraft ID
            let chunk_pos = position.to_chunk(); // Convert position to chunk coordinates

            // Create a DataBundle to send broadcasts
            let mut data_bundle = DataBundle::new(compose, system);

            debug!(
                "Syncing entity state for entity {} at position ({}, {}, {})",
                entity_id.0,
                position.x,
                position.y,
                position.z
            );

            //let position_delta = **position - **prev_position;

            let grounded = velocity.0.y == 0.0;
            let rotate_and_move_pkt = play::EntityPositionS2c {
                entity_id,
                position: position.as_dvec3(),
                yaw: ByteAngle::from_degrees(**yaw),
                pitch: ByteAngle::from_degrees(**pitch),
                on_ground: grounded,
            };
            
            //compose.broadcast(&rotate_and_move_pkt, system).send().unwrap();

            // Prepare the velocity update packet
            let velocity_pkt = play::EntityVelocityUpdateS2c {
                entity_id,
                velocity: velocity.to_packet_units(),
            };

            // Add both packets to the DataBundle
            data_bundle.add_packet(&rotate_and_move_pkt).unwrap();
            data_bundle.add_packet(&velocity_pkt).unwrap();

            entity
                .entity_view(world)
                .try_get::<&mut ActiveAnimation>(|animation| {

                        // Sync ActiveAnimation if it exists
                        for pkt in animation.packets(entity_id) {
                            data_bundle.add_packet(&pkt).unwrap();
                        }

                        animation.clear();

                    });
            data_bundle.broadcast_local(chunk_pos).unwrap();
        });
    }
}
