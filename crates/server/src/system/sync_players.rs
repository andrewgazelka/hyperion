use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{packets::play, VarInt};
use valence_text::{Color, IntoText};

use crate::{
    events::Gametick,
    global::Global,
    net::{Compressor, IoBuf, Packets},
    tracker::Prev,
    Vitals,
};
use crate::net::IoBufs;

#[allow(dead_code, reason = "todo")]
const HURT_SOUND: VarInt = VarInt(1018); // represents 1019
const REGENERATION: VarInt = VarInt(10);

#[allow(dead_code, reason = "todo")]
const ABSORPTION: VarInt = VarInt(22);
const SURVIVAL: f32 = 0.0;
const SPECTATOR: f32 = 3.0;

#[derive(Query)]
pub struct SyncPlayersQuery<'a> {
    id: EntityId,
    prev_vitals: &'a mut Prev<Vitals>,
    vitals: &'a mut Vitals,
    packets: &'a mut Packets,
}

#[instrument(skip_all)]
pub fn sync_players(
    gametick: ReceiverMut<Gametick>,
    global: Single<&Global>,
    mut fetcher: Fetcher<SyncPlayersQuery>,
    mut compressor: Single<&mut Compressor>,
    mut io: Single<&mut IoBufs>,
) {
    let tick = global.tick;

    let mut gametick = gametick.event;

    let scratch = &mut gametick.scratch;

    let scratch = scratch.one();
    let compressor = compressor.one();
    
    let mut io = io.one();

    fetcher.iter_mut().for_each(|query| {
        let entity_id = VarInt(query.id.index().0 as i32);
        let vitals = query.vitals;
        let packets = query.packets;

        let mut previous = &mut **query.prev_vitals;
        let mut current = vitals;

        match (&mut previous, &mut current) {
            (
                Vitals::Alive {
                    health: previous_health,
                    regeneration: previous_regeneration,
                    ..
                },
                Vitals::Alive {
                    health: current_health,
                    regeneration: current_regeneration,
                    ..
                },
            ) => {
                if (*previous_health - *current_health).abs() > f32::EPSILON {
                    // TODO: Sync absorption hearts

                    let _ = packets.append(
                        &play::HealthUpdateS2c {
                            health: *current_health,
                            food: VarInt(20),
                            food_saturation: 5.0,
                        },
                        &mut io,
                        scratch,
                        compressor,
                    );
                }

                // // TODO: Adding these effects don't work for some reason
                // if previous_absorption.end_tick != current_absorption.end_tick {
                //     let _ = packets.append(
                //         &play::EntityStatusEffectS2c {
                //             entity_id,
                //             effect_id: ABSORPTION,
                //             amplifier: 0,
                //             duration: dbg!(VarInt((current_absorption.end_tick - tick) as i32)),
                //             flags: play::entity_status_effect_s2c::Flags::new()
                //                 .with_show_icon(true),
                //             factor_codec: None,
                //         },
                //         &mut io,
                //     );
                // }

                if previous_regeneration.end_tick != current_regeneration.end_tick {
                    let _ = packets.append(
                        &play::EntityStatusEffectS2c {
                            entity_id,
                            effect_id: REGENERATION,
                            amplifier: 1,
                            duration: dbg!(VarInt((current_regeneration.end_tick - tick) as i32)),
                            flags: play::entity_status_effect_s2c::Flags::new()
                                .with_show_icon(true),
                            factor_codec: None,
                        },
                        &mut io,
                        scratch,
                        compressor,
                    );
                }
            }
            (Vitals::Alive { .. }, Vitals::Dead { respawn_tick }) => {
                let _ = packets.append(
                    &play::GameStateChangeS2c {
                        kind: play::game_state_change_s2c::GameEventKind::ChangeGameMode,
                        value: SPECTATOR,
                    },
                    &mut io,
                    scratch,
                    compressor,
                );
                // The title is repeatedly sent so it doesn't fade away after a few seconds
                let _ = packets.append(
                    &play::TitleS2c {
                        title_text: "YOU DIED!".into_text().color(Color::RED).into(),
                    },
                    &mut io,
                    scratch,
                    compressor,
                );

                let seconds_remaining = (*respawn_tick - tick) as f32 / 20.0;
                let _ = packets.append(
                    &play::SubtitleS2c {
                        subtitle_text: ("Respawning in ".into_text()
                            + format!("{seconds_remaining:.2}").color(Color::RED)
                            + " seconds")
                            .into(),
                    },
                    &mut io,
                    scratch,
                    compressor,
                );

                // packets
                //     .append(
                //         &play::PlaySoundS2c {
                //             id: SoundId::Reference { id: HURT_SOUND },
                //             category: SoundCategory::Player,
                //             position: (pose.position * 8.00).as_ivec3(),
                //             volume: 1.0,
                //             pitch: 1.0,
                //             seed: 0,
                //         },
                //         &mut io,
                //     )
                //     .unwrap();
            }
            (Vitals::Dead { .. }, Vitals::Alive { health, .. }) => {
                let _ = packets.append(
                    &play::ClearTitleS2c { reset: true },
                    &mut io,
                    scratch,
                    compressor,
                );
                let _ = packets.append(
                    &play::GameStateChangeS2c {
                        kind: play::game_state_change_s2c::GameEventKind::ChangeGameMode,
                        value: SURVIVAL,
                    },
                    &mut io,
                    scratch,
                    compressor,
                );
                let _ = packets.append(
                    &play::HealthUpdateS2c {
                        health: *health,
                        food: VarInt(20),
                        food_saturation: 5.0,
                    },
                    &mut io,
                    scratch,
                    compressor,
                );
                // TODO: Update absorption and regeneration
            }
            (Vitals::Dead { .. }, Vitals::Dead { respawn_tick }) => {
                let seconds_remaining = (*respawn_tick - tick) as f32 / 20.0;
                let _ = packets.append(
                    &play::SubtitleS2c {
                        subtitle_text: ("Respawning in ".into_text()
                            + format!("{seconds_remaining:.2}").color(Color::RED)
                            + " seconds")
                            .into(),
                    },
                    &mut io,
                    scratch,
                    compressor,
                );
            }
        }

        *previous = *current;
    });
}
