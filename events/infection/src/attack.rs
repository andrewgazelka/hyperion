use std::{
    borrow::Cow,
    time::{Duration, Instant},
};

use flecs_ecs::{
    core::{
        flecs, EntityViewGet, IdOperations, QueryBuilderImpl, SystemAPI, TableIter,
        TermBuilderImpl, World,
    },
    macros::{observer, system, Component},
    prelude::Module,
};
use hyperion::{
    net::{Compose, NetworkStreamRef},
    simulation::{
        blocks::{Blocks, EntityAndSequence},
        event,
        metadata::Metadata,
        Health, Player, Position,
    },
    storage::EventQueue,
    system_registry::SystemId,
    valence_protocol::{
        ident,
        math::{DVec3, IVec3},
        packets::{play, play::entity_attributes_s2c::AttributeProperty},
        sound::{SoundCategory, SoundId},
        text::IntoText,
        BlockPos, BlockState, ItemStack, Particle, VarInt,
    },
};
use hyperion_inventory::PlayerInventory;
use hyperion_scheduled::Scheduled;
use tracing::trace_span;

#[derive(Component)]
pub struct AttackModule;

#[derive(Component, Default, Copy, Clone, Debug)]
pub struct ImmuneUntil {
    tick: i64,
}

impl Module for AttackModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        // world.set(PendingDestruction::default());

        world
            .observer::<flecs::OnAdd, ()>()
            .with::<Player>()
            .each_entity(|it, _| {
                it.set(ImmuneUntil::default());
            });

        system!("handle_attacks", world, &mut EventQueue<event::AttackEntity>($), &Compose($))
            .multi_threaded()
            .each_iter(
                move |it: TableIter<'_, false>,
                      _,
                      (mut event_queue, compose): (
                    &mut EventQueue<event::AttackEntity>,
                    &Compose,
                )| {
                    const IMMUNE_TICK_DURATION: i64 = 10;
                    const DAMAGE: f32 = 1.0;

                    let span = trace_span!("handle_attacks");
                    let _enter = span.enter();

                    let current_tick = compose.global().tick;

                    let world = it.world();

                    for event in event_queue.drain() {
                        let target = world.entity_from_id(event.target);
                        let origin = world.entity_from_id(event.origin);

                        target.get::<(&mut ImmuneUntil, &mut Health, &mut Metadata, &Position)>(
                            |(immune_until, health, metadata, position)| {
                                if immune_until.tick > current_tick {
                                    return;
                                }

                                immune_until.tick = current_tick + IMMUNE_TICK_DURATION;
                                health.normal -= DAMAGE;

                                metadata.health(health.normal);

                                let entity_id = VarInt(event.target.0 as i32);

                                let pkt = play::EntityDamageS2c {
                                    entity_id,
                                    source_type_id: Default::default(),
                                    source_cause_id: Default::default(),
                                    source_direct_id: Default::default(),
                                    source_pos: None,
                                };

                                compose.broadcast(&pkt, SystemId(999)).send(&world).unwrap();

                                // let pkt = play::EntityAttributesS2c {
                                //     entity_id,
                                //     properties: vec![
                                //         AttributeProperty {
                                //             key: (),
                                //             value: 0.0,
                                //             modifiers: vec![],
                                //         }
                                //     ],
                                // }

                                // Play a sound when an entity is damaged
                                let ident = ident!("minecraft:entity.player.hurt");
                                let pkt = play::PlaySoundS2c {
                                    id: SoundId::Direct {
                                        id: ident.into(),
                                        range: None,
                                    },
                                    position: (position.position * 8.0).as_ivec3(),
                                    volume: 1.0,
                                    pitch: 1.0,
                                    seed: fastrand::i64(..),
                                    category: SoundCategory::Player,
                                };
                                compose.broadcast(&pkt, SystemId(999)).send(&world).unwrap();
                            },
                        );
                    }
                },
            );
    }
}
