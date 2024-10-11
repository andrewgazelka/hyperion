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
    net::{Compose, NetworkStreamRef},
    simulation::{
        blocks::{Blocks, EntityAndSequence},
        event,
    },
    storage::EventQueue,
    system_registry::SystemId,
    valence_protocol::{
        ident,
        math::{DVec3, IVec3},
        packets::play,
        sound::{SoundCategory, SoundId},
        text::IntoText,
        BlockPos, BlockState, ItemStack, Particle, VarInt,
    },
};
use hyperion_inventory::PlayerInventory;
use hyperion_scheduled::Scheduled;
use tracing::trace_span;

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
                        let ident = ident!("minecraft:block.stone.break");
                        let pkt = play::PlaySoundS2c {
                            id: SoundId::Direct { id: ident.into(), range: None },
                            position: (center_block * 8.0).as_ivec3(),
                            volume: 0.35,
                            pitch: (stage as f32).mul_add(0.1, 1.0),
                            seed: 0,
                            category: SoundCategory::Block,
                        };
                        compose.broadcast(&pkt, SystemId(999))
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
                            offset: Default::default(),
                            max_speed: 0.0,
                            count: 0,
                        };

                        compose.broadcast(&particle_packet, SystemId(999))
                            .send(&world)
                            .unwrap();

                            let ident = ident!("minecraft:entity.zombie.break_wooden_door");
                            let pkt = play::PlaySoundS2c {
                                id: SoundId::Direct { id: ident.into(), range: None },
                                position: (center_block * 8.0).as_ivec3(),
                                volume: 1.0,
                                pitch: 0.8,
                                seed: fastrand::i64(..),// random for seed variation
                                category: SoundCategory::Block,
                            };
                            compose.broadcast(&pkt, SystemId(999))
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
                        let position = IVec3::new(position.x << 3, position.y << 3, position.z << 3);


                        let ident = ident!("minecraft:block.note_block.harp");
                        // Send a note sound when breaking a block
                        let pkt = play::PlaySoundS2c {
                            id: SoundId::Direct { id: ident.into(), range: None },
                            position,
                            volume: 1.0,
                            pitch: 1.0,
                            seed: 0,
                            category: SoundCategory::Block,
                        };
                        compose.unicast(&pkt, net, system_id, &world).unwrap();

                        inventory.try_add_item(diff);
                    });
                }
            });

        const TOTAL_DESTRUCTION_TIME: Duration = Duration::from_secs(30);

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
                            let delay = TOTAL_DESTRUCTION_TIME / 10 * stage as u32;
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
    }
}
