use std::{
    borrow::Cow,
    time::{Duration, Instant},
};

use flecs_ecs::{
    core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    net::{agnostic, Compose, NetworkStreamRef},
    simulation::{
        blocks::{Blocks, EntityAndSequence},
        event,
    },
    storage::EventQueue,
    system_registry::SystemId,
    valence_protocol::{
        block::{PropName, PropValue},
        ident,
        math::{DVec3, IVec3, Vec3},
        packets::play,
        text::IntoText,
        BlockPos, BlockState, ItemStack, Particle, VarInt,
    },
};
use hyperion_inventory::PlayerInventory;
use hyperion_scheduled::Scheduled;
use tracing::{error, trace_span};

#[derive(Component)]
pub struct BlockModule;

pub struct SetLevel {
    pub position: IVec3,
    pub sequence: i32,
    pub stage: u8,
}

#[derive(Default, Component)]
pub struct PendingDestruction {
    pub destroy_at: Scheduled<Instant, IVec3>,
    pub set_level_at: Scheduled<Instant, SetLevel>,
}

impl Module for BlockModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        const TOTAL_DESTRUCTION_TIME: Duration = Duration::from_secs(30);

        world.set(PendingDestruction::default());

        system!("handle_pending_air", world, &mut PendingDestruction($), &mut Blocks($), &Compose($))
            .multi_threaded()
            .each_iter(
                move |
                    it: TableIter<'_, false>,
                      _,
                      (pending_air, blocks, compose): (&mut PendingDestruction, &mut Blocks, &Compose)| {
                    let now = Instant::now();
                    let world = it.world();
                    for SetLevel { position, sequence, stage } in pending_air.set_level_at.pop_until(&now) {
                        let packet = play::BlockBreakingProgressS2c {
                            entity_id: VarInt(sequence),
                            position: BlockPos::new(position.x, position.y, position.z),
                            destroy_stage: stage,
                        };
                        compose.broadcast(&packet, SystemId(999))
                            .send(&world)
                            .unwrap();

                        let center_block = position.as_dvec3() + DVec3::splat(0.5);
                        let sound = agnostic::sound(
                            ident!("minecraft:block.stone.break"),
                            center_block.as_vec3(),
                        ).volume(0.35)
                            .pitch(f32::from(stage).mul_add(0.1, 1.0))
                            .build();

                        compose.broadcast(&sound, SystemId(999))
                            .send(&world)
                            .unwrap();
                    }
                    for position in pending_air.destroy_at.pop_until(&now) {
                        // Play particle effect for block destruction
                        let center_block = position.as_dvec3() + DVec3::splat(0.5);

                        let particle_packet = play::ParticleS2c {
                            particle: Cow::Owned(Particle::Explosion),
                            long_distance: false,
                            position: center_block,
                            offset: Vec3::default(),
                            max_speed: 0.0,
                            count: 0,
                        };

                        compose.broadcast(&particle_packet, SystemId(999))
                            .send(&world)
                            .unwrap();

                            let sound = agnostic::sound(
                                ident!("minecraft:entity.zombie.break_wooden_door"),
                                center_block.as_vec3(),
                            ).volume(1.0)
                                .pitch(0.8)
                                .seed(fastrand::i64(..))
                                .build();

                            compose.broadcast(&sound, SystemId(999))
                                .send(&world)
                                .unwrap();

                        blocks.set_block(position, BlockState::AIR).unwrap();
                    }
                },
            );

        // todo: this is a hack. We want the system ID to be automatically assigned based on the location of the system.
        let system_id = SystemId(8);

        system!("handle_destroyed_blocks", world, &mut Blocks($), &mut EventQueue<event::DestroyBlock>($), &Compose($))
            .multi_threaded()
            .each_iter(move |it: TableIter<'_, false>, _, (mc, event_queue, compose): (&mut Blocks, &mut EventQueue<event::DestroyBlock>, &Compose)| {
                let span = trace_span!("handle_blocks");
                let _enter = span.enter();
                let world = it.world();


                for event in event_queue.drain() {
                    let Ok(previous) = mc.set_block(event.position, BlockState::AIR) else {
                        return;
                    };

                    let from = event.from;
                    let from_entity = world.entity_from_id(from);
                    from_entity.get::<(&NetworkStreamRef, &mut PlayerInventory)>(|(&net, inventory)| {
                        mc.to_confirm.push(EntityAndSequence {
                            entity: event.from,
                            sequence: event.sequence,
                        });


                        let previous_kind = previous.to_kind().to_item_kind();
                        let diff = ItemStack::new(previous.to_kind().to_item_kind(), 1, None);
                        // Create a message about the broken block
                        let msg = format!("previous {previous:?} → {previous_kind:?}");

                        let pkt = play::GameMessageS2c {
                            chat: msg.into_cow_text(),
                            overlay: false,
                        };

                        // Send the message to the player
                        compose.unicast(&pkt, net, system_id, &world).unwrap();

                        let position = event.position;

                        let sound = agnostic::sound(
                            ident!("minecraft:block.note_block.harp"),
                            position.as_vec3() + Vec3::splat(0.5),
                        ).volume(1.0)
                            .pitch(1.0)
                            .build();

                        compose.unicast(&sound, net, system_id, &world).unwrap();

                        inventory.try_add_item(diff);
                    });
                }
            });

        system!("handle_placed_blocks", world, &mut Blocks($), &mut EventQueue<event::PlaceBlock>($), &mut PendingDestruction($))
            .multi_threaded()
            .each_iter(move |_it: TableIter<'_, false>, _, (mc, event_queue, pending_air): (&mut Blocks, &mut EventQueue<event::PlaceBlock>, &mut PendingDestruction)| {
                let span = trace_span!("handle_placed_blocks");
                let _enter = span.enter();
                for event in event_queue.drain() {
                    let position = event.position;

                    mc.set_block(position, event.block).unwrap();

                    pending_air.destroy_at.schedule(Instant::now() + TOTAL_DESTRUCTION_TIME, position);

                    {
                        let sequence = fastrand::i32(..);
                        // Schedule destruction stages 0 through 9
                        for stage in 0_u8..=10 { // 10 represents no animation
                            let delay = TOTAL_DESTRUCTION_TIME / 10 * u32::from(stage);
                            pending_air.set_level_at.schedule(
                                Instant::now() + delay,
                                SetLevel {
                                    position,
                                    sequence,
                                    stage,
                                },
                            );
                        }
                    }
                    mc.to_confirm.push(EntityAndSequence {
                        entity: event.from,
                        sequence: event.sequence,
                    });
                }
            });

        system!("handle_toggled_doors", world, &mut Blocks($), &mut EventQueue<event::ToggleDoor>($))
            .multi_threaded()
            .each_iter(move |_it: TableIter<'_, false>, _, (mc, event_queue): (&mut Blocks, &mut EventQueue<event::ToggleDoor>)| {
                let span = trace_span!("handle_toggled_doors");
                let _enter = span.enter();
                for event in event_queue.drain() {
                    let position = event.position;

                    // The block is fetched again instead of sending the expected block state
                    // through the ToggleDoor event to avoid potential duplication bugs if the
                    // ToggleDoor event is sent, the door is broken, and the ToggleDoor event is
                    // processed
                    let Some(door) = mc.get_block(position) else { continue };
                    let Some(open) = door.get(PropName::Open) else { continue };

                    // Toggle the door state
                    let open = match open {
                        PropValue::False => PropValue::True,
                        PropValue::True => PropValue::False,
                        _ => {
                            error!("Door property 'Open' must be either 'True' or 'False'");
                            continue;
                        }
                    };

                    let door = door.set(PropName::Open, open);
                    mc.set_block(position, door).unwrap();

                    // Vertical doors (as in doors that are not trapdoors) need to have the other
                    // half of the door updated.
                    let other_half_position = match door.get(PropName::Half) {
                        Some(PropValue::Upper) => Some(position - IVec3::new(0, 1, 0)),
                        Some(PropValue::Lower) => Some(position + IVec3::new(0, 1, 0)),
                        Some(_) => {
                            error!("Door property 'Half' must be either 'Upper' or 'Lower'");
                            continue;
                        },
                        None => None
                    };

                    if let Some(other_half_position) = other_half_position {
                        let Some(other_half) = mc.get_block(other_half_position) else {
                            error!("Could not find other half of door");
                            continue;
                        };

                        let other_half = other_half.set(PropName::Open, open);
                        mc.set_block(other_half_position, other_half).unwrap();
                    }

                    mc.to_confirm.push(EntityAndSequence {
                        entity: event.from,
                        sequence: event.sequence,
                    });
                }
            });
    }
}
