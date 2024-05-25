use evenio::prelude::*;
use tracing::instrument;

use crate::{components::Singleton, event::Gametick, global::Global, net::Compose};

#[instrument(skip_all, level = "trace")]
pub fn send_time(_: Receiver<Gametick>, compose: Compose, global: Singleton<Global>) {
    let tick = global.tick;
    let time_of_day = tick % 24000;

    // Only sync with the client every 5 seconds
    if tick % (20 * 5) == 0 {
        let pkt = valence_protocol::packets::play::WorldTimeUpdateS2c {
            world_age: tick,
            time_of_day,
        };

        compose.broadcast(&pkt).unwrap();
    }
}

#[instrument(skip_all, level = "trace")]
pub fn update_time(_: Receiver<Gametick>, mut global: Single<&mut Global>) {
    global.tick += 1;
}
