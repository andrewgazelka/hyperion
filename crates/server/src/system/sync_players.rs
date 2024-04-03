use evenio::{prelude::*, rayon::prelude::*};
use tracing::instrument;
use valence_protocol::{packets::play, sound::SoundCategory, VarInt};
use valence_text::{Color, IntoText};

use crate::{BroadcastPackets, Player, PlayerState};

const HURT_SOUND: VarInt = VarInt(520);
const REGENERATION: VarInt = VarInt(10);
const ABSORPTION: VarInt = VarInt(22);
const SURVIVAL: f32 = 0.0;
const SPECTATOR: f32 = 3.0;
const UPDATE_RESPAWN_TIMER_INTERVAL: u64 = 5;

#[instrument(skip_all)]
pub fn sync_players(
    broadcast_packets: Receiver<BroadcastPackets>,
    mut fetcher: Fetcher<(EntityId, &mut Player)>,
) {
    let tick = broadcast_packets.event.tick;
    fetcher.par_iter_mut().for_each(|(id, player)| {
        let entity_id = VarInt(id.index().0 as i32);
        match (player.state.previous(), player.state.current()) {
            (
                PlayerState::Alive {
                    health: previous_health,
                    absorption: previous_absorption,
                    regeneration: previous_regeneration,
                },
                PlayerState::Alive {
                    health: current_health,
                    absorption: current_absorption,
                    regeneration: current_regeneration,
                },
            ) => {
                if previous_health != current_health {
                    // TODO: Sync absorption hearts

                    let _ = player.packets.writer.send_packet(&play::HealthUpdateS2c {
                        health: *current_health,
                        food: VarInt(20),
                        food_saturation: 5.0,
                    });

                    if current_health < previous_health {
                        let _ = player
                            .packets
                            .writer
                            .send_packet(&play::PlaySoundFromEntityS2c {
                                id: HURT_SOUND,
                                // TODO: Check if these hurt sound settings are correct
                                category: SoundCategory::Player,
                                entity_id,
                                volume: 1.0,
                                pitch: 1.0,
                                seed: 0,
                            });
                    }
                }

                // TODO: Adding these effects don't work for some reason
                if previous_absorption.end_tick != current_absorption.end_tick {
                    let _ = player
                        .packets
                        .writer
                        .send_packet(&play::EntityStatusEffectS2c {
                            entity_id,
                            effect_id: ABSORPTION,
                            amplifier: 0,
                            duration: dbg!(VarInt(
                                (current_absorption.end_tick - tick.number) as i32
                            )),
                            flags: play::entity_status_effect_s2c::Flags::new()
                                .with_show_icon(true),
                            factor_codec: None,
                        });
                }

                if previous_regeneration.end_tick != current_regeneration.end_tick {
                    let _ = player
                        .packets
                        .writer
                        .send_packet(&play::EntityStatusEffectS2c {
                            entity_id,
                            effect_id: REGENERATION,
                            amplifier: 1,
                            duration: dbg!(VarInt(
                                (current_regeneration.end_tick - tick.number) as i32
                            )),
                            flags: play::entity_status_effect_s2c::Flags::new()
                                .with_show_icon(true),
                            factor_codec: None,
                        });
                }
            }
            (PlayerState::Alive { .. }, PlayerState::Dead { respawn_tick }) => {
                let _ = player
                    .packets
                    .writer
                    .send_packet(&play::GameStateChangeS2c {
                        kind: play::game_state_change_s2c::GameEventKind::ChangeGameMode,
                        value: SPECTATOR,
                    });
                // TitleS2c is needed to stop the title from fading away after a few seconds
                // TODO: Stop this from flickering
                let _ = player.packets.writer.send_packet(&play::TitleS2c {
                    title_text: "YOU DIED!".into_text().color(Color::RED).into(),
                });
                let seconds_remaining = (respawn_tick - tick.number) as f32 / 20.0;
                let _ = player.packets.writer.send_packet(&play::SubtitleS2c {
                    subtitle_text: ("Respawning in ".into_text()
                        + format!("{seconds_remaining:.2}").color(Color::RED)
                        + "seconds")
                        .into(),
                });
            }
            (PlayerState::Dead { .. }, PlayerState::Alive { health, .. }) => {
                let _ = player
                    .packets
                    .writer
                    .send_packet(&play::ClearTitleS2c { reset: true });
                let _ = player
                    .packets
                    .writer
                    .send_packet(&play::GameStateChangeS2c {
                        kind: play::game_state_change_s2c::GameEventKind::ChangeGameMode,
                        value: SURVIVAL,
                    });
                let _ = player.packets.writer.send_packet(&play::HealthUpdateS2c {
                    health: *health,
                    food: VarInt(20),
                    food_saturation: 5.0,
                });
                // TODO: Update absorption and regeneration
            }
            (PlayerState::Dead { .. }, PlayerState::Dead { respawn_tick }) => {
                let ticks_remaining = respawn_tick - tick.number;
                if ticks_remaining % UPDATE_RESPAWN_TIMER_INTERVAL == 0 {
                    let seconds_remaining = (respawn_tick - tick.number) as f32 / 20.0;
                    // The title is repeatedly sent so it doesn't fade away after a few seconds
                    let _ = player.packets.writer.send_packet(&play::TitleS2c {
                        title_text: "YOU DIED!".into_text().color(Color::RED).into(),
                    });
                    let _ = player.packets.writer.send_packet(&play::SubtitleS2c {
                        subtitle_text: ("Respawning in ".into_text()
                            + format!("{seconds_remaining:.2}").color(Color::RED)
                            + "seconds")
                            .into(),
                    });
                }
            }
        }

        player.state.update_previous();
    });
}
