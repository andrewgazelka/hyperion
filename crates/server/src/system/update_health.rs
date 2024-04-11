use evenio::{prelude::*, rayon::prelude::*};
use tracing::instrument;

use crate::{global::Global, Gametick, Player, PlayerState, Tracker};

/// Interval to regenerate half a heart from having a full hunger bar measured in ticks. All players
/// are assumed to have a full hunger bar with no saturation.
const HUNGER_INTERVAL: u64 = 80;

/// Interval to regenerate half a heart from the regeneration potion effect measured in ticks. This
/// assumes that all regeneration is at level 2, which is true since only golden apples are used.
const REGENERATION_INTERVAL: u64 = 25;

#[instrument(skip_all)]
pub fn update_health(
    _: Receiver<Gametick>,
    global: Single<&Global>,
    mut fetcher: Fetcher<&mut Player>,
) {
    let tick = global.tick.unsigned_abs();
    let hunger = tick % HUNGER_INTERVAL == 0;
    let regeneration = tick % REGENERATION_INTERVAL == 0;

    fetcher
        .par_iter_mut()
        .for_each(|player| match player.state.current().clone() {
            PlayerState::Alive {
                regeneration: regeneration_effect,
                ..
            } => {
                if hunger {
                    player.heal(tick, 1.0);
                }

                if regeneration && tick < regeneration_effect.end_tick {
                    player.heal(tick, 1.0);
                }
            }
            PlayerState::Dead { respawn_tick } => {
                if tick == respawn_tick {
                    // TODO: This code is really bad
                    let value = Tracker::<PlayerState>::default();

                    // TODO: Teleport player to spawn location after respawning
                    player.state.update(|state| {
                        *state = value.current().clone();
                    });
                }
            }
        });
}
