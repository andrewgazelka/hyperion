use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{packets::play, VarInt};

use crate::{
    event,
    event::Gametick,
    global::Global,
    net::{Compose, Packets},
    tracker::Prev,
    Vitals,
};

#[allow(dead_code, reason = "todo")]
const HURT_SOUND: VarInt = VarInt(1018); // represents 1019
const REGENERATION: VarInt = VarInt(10);

#[allow(dead_code, reason = "todo")]
const ABSORPTION: VarInt = VarInt(22);
const SURVIVAL: f32 = 0.0;

#[derive(Query)]
pub struct SyncPlayersQuery<'a> {
    id: EntityId,
    prev_vitals: &'a mut Prev<Vitals>,
    vitals: &'a mut Vitals,
    packets: &'a mut Packets,
}

#[instrument(skip_all)]
pub fn sync_players(
    _: Receiver<Gametick>,
    global: Single<&Global>,
    mut fetcher: Fetcher<SyncPlayersQuery>,
    s: Sender<event::Death>,
    compose: Compose,
) {
    let tick = global.tick;

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
                        &compose,
                    );
                }

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
                        &compose,
                    );
                }
            }
            (Vitals::Alive { .. }, Vitals::Dead) => {
                s.send_to(query.id, event::Death);
            }
            (Vitals::Dead, Vitals::Alive { health, .. }) => {
                let _ = packets.append(&play::ClearTitleS2c { reset: true }, &compose);
                let _ = packets.append(
                    &play::GameStateChangeS2c {
                        kind: play::game_state_change_s2c::GameEventKind::ChangeGameMode,
                        value: SURVIVAL,
                    },
                    &compose,
                );
                let _ = packets.append(
                    &play::HealthUpdateS2c {
                        health: *health,
                        food: VarInt(20),
                        food_saturation: 5.0,
                    },
                    &compose,
                );
                // TODO: Update absorption and regeneration
            }
            (Vitals::Dead, Vitals::Dead) => {}
        }

        *previous = *current;
    });
}
