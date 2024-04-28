use evenio::prelude::*;
use tracing::instrument;

use crate::{
    event::Gametick,
    global::Global,
    net::{Broadcast, Compose},
};

#[instrument(skip_all, level = "trace")]
pub fn update_time(
    _: Receiver<Gametick>,
    broadcast: Single<&Broadcast>,
    mut global: Single<&mut Global>,
    compose: Compose,
) {
    let tick = global.tick;
    let time_of_day = tick % 24000;

    // Only sync with the client every 5 seconds
    if tick % (20 * 5) == 0 {
        let pkt = valence_protocol::packets::play::WorldTimeUpdateS2c {
            world_age: tick,
            time_of_day,
        };

        broadcast.append(&pkt, &compose).unwrap();
    }

    // update the tick
    global.tick += 1;
}
