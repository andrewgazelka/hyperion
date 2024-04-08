use evenio::prelude::*;
use tracing::instrument;

use crate::{global::Global, singleton::encoder::Broadcast, Gametick};

#[instrument(skip_all, level = "trace")]
pub fn update_time(
    _: ReceiverMut<Gametick>,
    mut broadcast: Single<&mut Broadcast>,
    mut global: Single<&mut Global>,
) {
    let tick = global.tick;
    let time_of_day = tick % 24000;

    let pkt = valence_protocol::packets::play::WorldTimeUpdateS2c {
        world_age: tick,
        time_of_day,
    };

    broadcast.get_round_robin().append_packet(&pkt).unwrap();

    // update the tick
    global.tick += 1;
}
