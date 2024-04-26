use evenio::prelude::*;
use tracing::instrument;

use crate::{
    components::vitals::{Absorption, Regeneration},
    event::Gametick,
    global::Global,
    Vitals,
};

/// Interval to regenerate half a heart from having a full hunger bar measured in ticks. All players
/// are assumed to have a full hunger bar with no saturation.
const HUNGER_INTERVAL: i64 = 80;

/// Interval to regenerate half a heart from the regeneration potion effect measured in ticks. This
/// assumes that all regeneration is at level 2, which is true since only golden apples are used.
#[expect(unused, reason = "probably will be used in future")]
const REGENERATION_INTERVAL: i64 = 25;

#[derive(Query)]
pub struct UpdateHealthQuery<'a> {
    vitals: &'a mut Vitals,
}

#[instrument(skip_all)]
pub fn update_health(
    _: Receiver<Gametick>,
    global: Single<&Global>,
    mut fetcher: Fetcher<UpdateHealthQuery>,
) {
    let tick = global.tick;
    let hunger = tick % HUNGER_INTERVAL == 0;
    // let regeneration = tick % REGENERATION_INTERVAL == 0;

    fetcher.iter_mut().for_each(|query| {
        let vitals = query.vitals;
        match vitals {
            &mut Vitals::Alive { regeneration, .. } => {
                if hunger {
                    vitals.heal(1.0);
                }

                if tick < regeneration.end_tick {
                    vitals.heal(1.0);
                }
            }
            Vitals::Dead { respawn_tick } => {
                if tick == *respawn_tick {
                    // TODO: This code is really bad

                    // TODO: Teleport player to spawn location after respawning
                    *vitals = Vitals::Alive {
                        health: 20.0,
                        absorption: Absorption::default(),
                        regeneration: Regeneration::default(),
                    }
                }
            }
        }
    });
}
