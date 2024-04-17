use evenio::prelude::*;
use tracing::instrument;

use crate::{events::Gametick, global::Global, singleton::broadcast::BroadcastBuf};

#[instrument(skip_all, level = "trace")]
pub fn update_time(
    _: ReceiverMut<Gametick>,
    mut broadcast: Single<&mut BroadcastBuf>,
    mut global: Single<&mut Global>,
) {
    let tick = global.tick;
    let time_of_day = tick % 24000;

    // Only sync with the client every 5 seconds
    if tick % (20 * 5) == 0 {
        let pkt = valence_protocol::packets::play::WorldTimeUpdateS2c {
            world_age: tick,
            time_of_day,
        };

        broadcast.get_round_robin().append_packet(&pkt).unwrap();
    }

    // update the tick
    global.tick += 1;
}
