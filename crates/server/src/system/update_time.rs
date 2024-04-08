use evenio::prelude::*;
use tracing::instrument;

use crate::{
    global::Global,
    singleton::encoder::{Broadcast, PacketMetadata},
    Gametick,
};

#[instrument(skip_all, level = "trace")]
pub fn update_time(
    _: ReceiverMut<Gametick>,
    encoder: Single<&mut Broadcast>,
    global: Single<&mut Global>,
) {
    let global = global.0;
    let encoder = encoder.0;

    let tick = global.tick;
    let time_of_day = tick % 24000;

    let pkt = valence_protocol::packets::play::WorldTimeUpdateS2c {
        world_age: tick,
        time_of_day,
    };

    encoder
        .get_round_robin()
        .append_packet(&pkt, PacketMetadata::DROPPABLE)
        .unwrap();

    // update the tick
    global.tick += 1;
}
