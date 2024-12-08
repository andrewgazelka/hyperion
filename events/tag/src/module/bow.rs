use flecs_ecs::{
    core::{Entity, EntityView, EntityViewGet, World, WorldProvider},
    prelude::*,
};
use hyperion::{
    ItemKind, ItemStack,
    glam::Vec3,
    simulation::{
        Pitch, Position, Spawn, Uuid, Velocity, Yaw, bow::BowCharging, entity_kind::EntityKind,
        event, get_direction_from_rotation,
    },
    storage::EventQueue,
};
use hyperion_inventory::PlayerInventory;
use tracing::{debug, info_span};

#[derive(Component)]
pub struct BowModule;

impl Module for BowModule {
    fn module(world: &World) {
        system!(
            "handle_bow_release",
            world,
            &mut EventQueue<event::ReleaseUseItem>,
            &mut PlayerInventory,
            &Position,
            &Yaw,
            &Pitch,
        )
        .term_at(0u32)
        .singleton()
        .kind::<flecs::pipeline::PostUpdate>()
        .each_iter(
            move |it, _, (event_queue, inventory, position, yaw, pitch)| {
                let span = info_span!("handle_bow_release");
                let _enter = span.enter();
                let _system = it.system();
                let world = it.world();

                for event in event_queue.drain() {
                    if event.item != ItemKind::Bow {
                        continue;
                    }

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
                                    .set(slot, ItemStack::new(item.item, count, item.nbt.clone()))
                                    .unwrap();
                            }
                            has_arrow = true;
                            break;
                        }
                    }

                    if !has_arrow {
                        continue;
                    }

                    // Get how charged the bow is
                    let charge = event
                        .from
                        .entity_view(world)
                        .try_get::<&BowCharging>(|charging| {
                            let charge = charging.get_charge();
                            event.from
                                .entity_view(world)
                                .remove::<BowCharging>();
                            charge
                        })
                        .unwrap_or(0.0);

                    debug!("charge: {charge}");

                    // Calculate the direction vector from the player's rotation
                    let direction = get_direction_from_rotation(**yaw, **pitch);
                    // Calculate the velocity of the arrow based on the charge (3.0 is max velocity)
                    let velocity = direction * (charge * 3.0);

                    let spawn_pos =
                        Vec3::new(position.x, position.y + 1.62, position.z) + direction * 0.5;

                    debug!(
                        "Arrow velocity: ({}, {}, {})",
                        velocity.x, velocity.y, velocity.z
                    );

                    debug!("Arrow Yaw: {}, Arrow Pitch: {}", **yaw, **pitch);

                    // Spawn arrow
                    world
                        .entity()
                        .add_enum(EntityKind::Arrow)
                        .set(Uuid::new_v4())
                        .set(Position::new(spawn_pos.x, spawn_pos.y, spawn_pos.z))
                        .set(Velocity::new(velocity.x, velocity.y, velocity.z))
                        .set(Pitch::new(**pitch))
                        .set(Yaw::new(**yaw))
                        .enqueue(Spawn);
                }
            },
        );
    }
}
