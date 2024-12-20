use flecs_ecs::{
    core::{EntityViewGet, World},
    prelude::*,
};
use glam::I16Vec2;
use hyperion::{
    glam::Vec3, net::Compose, simulation::{
        bow::BowCharging, entity_kind::EntityKind, event, get_direction_from_rotation, metadata::living_entity::{ArrowsInEntity, HandStates}, Pitch, Position, Spawn, Uuid, Velocity, Yaw
    }, storage::{EventQueue, Events}, valence_protocol::packets::play, ItemKind, ItemStack
};
use hyperion_inventory::PlayerInventory;
use hyperion_utils::EntityExt;
use tracing::debug;
use valence_protocol::VarInt;

#[derive(Component)]
pub struct BowModule;

#[derive(Component)]
pub struct Owner {
    entity: Entity,
}

impl Owner {
    pub fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

impl Module for BowModule {
    fn module(world: &World) {
        world.component::<Owner>();

        system!(
            "handle_bow_release",
            world,
            &mut EventQueue<event::ReleaseUseItem>($),
        )
        .singleton()
        .kind::<flecs::pipeline::PostUpdate>()
        .each_iter(move |it, _, event_queue| {
            let _system = it.system();
            let world = it.world();

            for event in event_queue.drain() {
                if event.item != ItemKind::Bow {
                    continue;
                }

                let player = world.entity_from_id(event.from);

                #[allow(clippy::excessive_nesting)]
                player.get::<(&mut PlayerInventory, &Position, &Yaw, &Pitch)>(
                    |(inventory, position, yaw, pitch)| {
                        // check if the player has enough arrows in their inventory
                        let items: Vec<(u16, &ItemStack)> = inventory.items().collect();
                        let mut has_arrow = false;
                        for (slot, item) in items {
                            if item.item == ItemKind::Arrow && item.count >= 1 {
                                let count = item.count - 1;
                                if count == 0 {
                                    inventory.set(slot, ItemStack::EMPTY).unwrap();
                                } else {
                                    inventory
                                        .set(
                                            slot,
                                            ItemStack::new(item.item, count, item.nbt.clone()),
                                        )
                                        .unwrap();
                                }
                                has_arrow = true;
                                break;
                            }
                        }

                        if !has_arrow {
                            return;
                        }

                        // Get how charged the bow is
                        let charge = event
                            .from
                            .entity_view(world)
                            .try_get::<&BowCharging>(|charging| {
                                let charge = charging.get_charge();
                                event.from.entity_view(world).remove::<BowCharging>();
                                charge
                            })
                            .unwrap_or(0.0);

                        // Calculate the direction vector from the player's rotation
                        let direction = get_direction_from_rotation(**yaw, **pitch);
                        // Calculate the velocity of the arrow based on the charge (3.0 is max velocity)
                        let velocity = direction * (charge * 3.0);

                        let spawn_pos =
                            Vec3::new(position.x, position.y + 1.62, position.z) + direction * 0.5;

                        // Spawn arrow
                        world
                            .entity()
                            .add_enum(EntityKind::Arrow)
                            .set(Uuid::new_v4())
                            .set(Position::new(spawn_pos.x, spawn_pos.y, spawn_pos.z))
                            .set(Velocity::new(velocity.x, velocity.y, velocity.z))
                            .set(Pitch::new(**pitch))
                            .set(Yaw::new(**yaw))
                            .set(Owner::new(*player))
                            .enqueue(Spawn);
                    },
                );
            }
        });

        system!(
            "arrow_entity_hit",
            world,
            &Compose($),
            &mut EventQueue<event::ProjectileEntityEvent>,
        )
        .singleton()
        .kind::<flecs::pipeline::PostUpdate>()
        .each_iter(move |it, _, (compose, event_queue)| {
            let system = it.system();
            let world = it.world();

            for event in event_queue.drain() {
                let (damage, owner, chunk_pos) = event
                    .projectile
                    .entity_view(world)
                    .get::<(&Velocity, &Owner)>(|(velocity, owner)| {
                        if owner.entity == event.client {
                            return (0.0, owner.entity, I16Vec2::ZERO);
                        }
                        let chunck_pos = event.client.entity_view(world).get::<&Position>(|pos| {
                            pos.to_chunk()
                        });
                        (velocity.0.length() * 2.0, owner.entity, chunck_pos)
                    });
                
                if damage == 0.0 && owner == event.client {
                    continue;
                }

                event
                    .client
                    .entity_view(world)
                    .get::<&mut ArrowsInEntity>(|arrows| {
                        arrows.0 += 1;
                    });
                
                let packet = play::EntitiesDestroyS2c {
                    entity_ids: vec![VarInt(event.projectile.minecraft_id() as i32)].into(),
                };
                compose.broadcast_local(&packet, chunk_pos, system).send().unwrap();

                event.projectile.entity_view(world).destruct();

                world.get::<&Events>(|events| {
                    events.push(
                        event::AttackEntity {
                            origin: owner,
                            target: event.client,
                            damage: damage,
                        },
                        &world,
                    );
                })
            }
        });

        system!(
            "arrow_block_hit",
            world,
            &mut EventQueue<event::ProjectileBlockEvent>,
        )
        .multi_threaded()
        .kind::<flecs::pipeline::PreStore>()
        .each_iter(move |it, _, event_queue| {
            let _system = it.system();
            let world = it.world();

            for event in event_queue.drain() {
                event
                    .projectile
                    .entity_view(world)
                    .get::<(&mut Position, &mut Velocity)>(|(position, velocity)| {
                        velocity.0 = Vec3::ZERO;
                        **position = event.collision.normal;
                    });
            }
        });
    }
}
