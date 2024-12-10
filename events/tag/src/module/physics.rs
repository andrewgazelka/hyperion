use std::fmt::Debug;

use flecs_ecs::{
    core::{Entity, EntityView, EntityViewGet, World, WorldGet},
    prelude::*,
};
use geometry::ray::Ray;
use glam::Vec3;
use hyperion::{
    net::{Compose, ConnectionId, DataBundle},
    simulation::{
        Pitch, Position, Velocity, Yaw,
        animation::{self, ActiveAnimation},
        blocks::Blocks,
        entity_kind::EntityKind,
    },
    valence_protocol::{ByteAngle, VarInt, packets::play},
};
use hyperion_inventory::PlayerInventory;
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

struct CheckCol {
    pos: Position,
    hitbox: Hitbox,
}

// Function to check for collision between two hitboxes using AABB in 3D
pub fn check_collision(hitbox_a: &CheckCol, hitbox_b: &CheckCol) -> bool {
    hitbox_a.pos.x < hitbox_b.pos.x + hitbox_b.hitbox.width
        && hitbox_a.pos.x + hitbox_a.hitbox.width > hitbox_b.pos.x
        && hitbox_a.pos.y < hitbox_b.pos.y + hitbox_b.hitbox.height
        && hitbox_a.pos.y + hitbox_a.hitbox.height > hitbox_b.pos.y
        && hitbox_a.pos.z < hitbox_b.pos.z + hitbox_b.hitbox.depth
        && hitbox_a.pos.z + hitbox_a.hitbox.depth > hitbox_b.pos.z
}

// Function to determine if a hitbox is grounded based on raycasting against blocks
pub fn is_grounded(hitbox: &Hitbox, blocks: &Blocks) -> bool {
    // Create a ray from the hitbox's position downwards
    let ray = Ray::new(
        Vec3::new(hitbox.width / 2.0, hitbox.height, hitbox.depth / 2.0),
        Vec3::new(0.0, -1.0, 0.0), // Direction pointing downward
    );

    // Check for collision with blocks using the first_collision method
    if let Some(_collision) = blocks.first_collision(ray, hitbox.height) {
        // If a collision is detected, the hitbox is considered grounded
        return true;
    }

    false // Hitbox is not grounded
}

// Function to update the velocity of a single entity based on grounded state
pub fn update_velocity(
    _entity: &Entity,
    hitbox: &Hitbox,
    velocity: &mut Velocity,
    blocks: &Blocks,
) {
    let grounded = is_grounded(hitbox, blocks);

    // Apply damping to velocity
    velocity.velocity.x *= 0.99;
    velocity.velocity.y *= 0.99;
    velocity.velocity.z *= 0.99;

    debug!(
        "Entity Grounded: {}, Velocity: ({}, {}, {})",
        grounded, velocity.velocity.x, velocity.velocity.y, velocity.velocity.z
    );

    // Apply gravity if not grounded
    if !grounded {
        velocity.velocity.y -= 0.08; // Gravity effect
        // Cap terminal velocity
        if velocity.velocity.y < -4.0 {
            velocity.velocity.y = -4.0;
        }
    } else {
        // Reset downward velocity if grounded
        velocity.velocity.y = 0.0;
    }
}

// Function to update the velocity and position of a single entity, and sync its state
pub fn update_entity_state(
    entity: &Entity,
    hitbox: &Hitbox,
    velocity: &mut Velocity,
    position: &mut Position,
    blocks: &Blocks,
) {
    // Update velocity based on grounded state
    update_velocity(entity, hitbox, velocity, blocks);

    // Update position based on velocity
    position.x += velocity.velocity.x;
    position.y += velocity.velocity.y;
    position.z += velocity.velocity.z;
}

// Function to sync entity state over the network
fn sync_entity_state(
    system: EntityView<'_>,
    entity: &Entity,
    compose: &Compose,
    position: &Position,
    velocity: &Velocity,
    yaw: &Yaw,
    pitch: &Pitch,
    active_animation: Option<&mut ActiveAnimation>,
) {
    let entity_id = VarInt(entity.minecraft_id()); // Assuming entity has a method to get its Minecraft ID
    let chunk_pos = position.to_chunk(); // Convert position to chunk coordinates

    // Create a DataBundle to send broadcasts
    let mut data_bundle = DataBundle::new(compose, system);

    // Prepare the position update packet
    let position_pkt = play::EntityPositionS2c {
        entity_id,
        position: position.as_dvec3(),
        yaw: ByteAngle::from_degrees(**yaw),
        pitch: ByteAngle::from_degrees(**pitch),
        on_ground: false, // Set to true if the entity is grounded
    };

    // Add the position update packet to the DataBundle
    data_bundle.add_packet(&position_pkt).unwrap();

    // Prepare the head yaw update packet
    let head_yaw_pkt = play::EntitySetHeadYawS2c {
        entity_id,
        head_yaw: ByteAngle::from_degrees(**yaw),
    };

    // Add the head yaw update packet to the DataBundle
    data_bundle.add_packet(&head_yaw_pkt).unwrap();

    // Prepare the velocity update packet
    let velocity_pkt = play::EntityVelocityUpdateS2c {
        entity_id,
        velocity: (*velocity).try_into().unwrap(), // Assuming velocity can be converted
    };

    // Add the velocity update packet to the DataBundle
    data_bundle.add_packet(&velocity_pkt).unwrap();

    // Sync ActiveAnimation if it exists
    if let Some(animation) = active_animation {
        for pkt in animation.packets(entity_id) {
            data_bundle.add_packet(&pkt).unwrap();
        }

        animation.clear();
    }
    data_bundle.broadcast_local(chunk_pos).unwrap();
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
                debug!("Physics module running for entity: {:?}", entity);

                let world = it.system().world();

                entity.entity_view(world).try_get::<&EntityKind>(|kind| {
                    debug!("EntityKind: {kind:?}");
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

                        update_entity_state(&entity, &hitbox, velocity, position, blocks);
                    });
                });
            });

        system!(
            "sync_entity_state",
            world,
            &Compose($),
            &Position,
            &Velocity,
            &Yaw,
            &Pitch,
        )
        .multi_threaded()
        .kind::<flecs::pipeline::PreStore>()
        .each_iter(|it, row, (compose, position, velocity, yaw, pitch)| {
            let world = it.system().world();
            let system = it.system();

            let entity = it.entity(row);

            entity
                .entity_view(world)
                .try_get::<&mut ActiveAnimation>(|animation| {
                    sync_entity_state(
                        system,
                        &entity,
                        compose,
                        position,
                        velocity,
                        yaw,
                        pitch,
                        Some(animation),
                    );
                })
                .unwrap_or({
                    sync_entity_state(
                        system, &entity, compose, position, velocity, yaw, pitch, None,
                    );
                });
        });
    }
}
