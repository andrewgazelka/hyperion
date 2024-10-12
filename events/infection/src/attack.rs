use flecs_ecs::{
    core::{flecs, EntityViewGet, QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    net::{Compose, NetworkStreamRef},
    simulation::{
        event, metadata::Metadata, EntityReaction, Health, Player, Position, PLAYER_SPAWN_POSITION,
    },
    storage::EventQueue,
    system_registry::SystemId,
    valence_protocol::{
        game_mode::OptGameMode,
        ident,
        packets::{play, play::player_position_look_s2c::PlayerPositionLookFlags},
        sound::{SoundCategory, SoundId},
        GameMode, VarInt,
    },
};
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
                      (event_queue, compose): (
                    &mut EventQueue<event::AttackEntity>,
                    &Compose,
                )| {
                    const IMMUNE_TICK_DURATION: i64 = 10;
                    const DAMAGE: f32 = 10.0;

                    let span = trace_span!("handle_attacks");
                    let _enter = span.enter();

                    let current_tick = compose.global().tick;

                    let world = it.world();

                    for event in event_queue.drain() {
                        let target = world.entity_from_id(event.target);
                        let origin = world.entity_from_id(event.origin);

                        let from_pos = origin.get::<&Position>(|pos| pos.position);

                        target.get::<(
                            &mut ImmuneUntil,
                            &mut Health,
                            &mut Metadata,
                            &mut Position,
                            &mut EntityReaction,
                            &NetworkStreamRef,
                        )>(
                            |(immune_until, health, metadata, position, reaction, io)| {
                                if immune_until.tick > current_tick {
                                    return;
                                }

                                immune_until.tick = current_tick + IMMUNE_TICK_DURATION;
                                health.normal -= DAMAGE;
                                if health.normal <= 0.0 {
                                    // send respawn packet

                                    let pkt = play::PlayerRespawnS2c {
                                        dimension_type_name: ident!("minecraft:overworld").into(),
                                        dimension_name: ident!("minecraft:overworld").into(),
                                        hashed_seed: 0,
                                        game_mode: GameMode::Adventure,
                                        previous_game_mode: OptGameMode::default(),
                                        is_debug: false,
                                        is_flat: false,
                                        copy_metadata: false,
                                        last_death_location: None,
                                        portal_cooldown: VarInt::default(),
                                    };
                                    position.position = PLAYER_SPAWN_POSITION;
                                    compose
                                        .unicast(&pkt, *io, SystemId(99), &world)
                                        .unwrap();
                                    health.normal = 20.0;
                                    metadata.health(20.0);
                                    return;
                                }
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

                                // Calculate velocity change based on attack direction
                                let this = position.position;
                                let other = from_pos;

                                let delta_x = other.x - this.x;
                                let delta_z = other.z - this.z;

                                if delta_x.abs() >= 0.01 || delta_z.abs() >= 0.01 {
                                    let dist_xz = delta_x.hypot(delta_z);
                                    let multiplier = 0.4;

                                    reaction.velocity /= 2.0;
                                    reaction.velocity.x -= delta_x / dist_xz * multiplier;
                                    reaction.velocity.y += multiplier;
                                    reaction.velocity.z -= delta_z / dist_xz * multiplier;

                                    reaction.velocity.y = reaction.velocity.y.min(0.4);
                                }
                            },
                        );
                    }
                },
            );
    }
}
