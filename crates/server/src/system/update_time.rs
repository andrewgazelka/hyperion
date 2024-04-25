use std::ops::DerefMut;

use evenio::prelude::*;
use tracing::instrument;

use crate::{
    events::Gametick,
    global::Global,
    net::{Broadcast, Compressor, IoBuf},
};

#[instrument(skip_all, level = "trace")]
pub fn update_time(
    gametick: ReceiverMut<Gametick>,
    mut broadcast: Single<&mut Broadcast>,
    mut global: Single<&mut Global>,
    mut io: Single<&mut IoBuf>,
    mut compressor: Single<&mut Compressor>,
) {
    let mut gametick = gametick.event;

    let gametick = &mut *gametick;

    let mut scratch = &mut gametick.scratch;

    let tick = global.tick;
    let time_of_day = tick % 24000;

    // Only sync with the client every 5 seconds
    if tick % (20 * 5) == 0 {
        let pkt = valence_protocol::packets::play::WorldTimeUpdateS2c {
            world_age: tick,
            time_of_day,
        };

        let scratch = scratch.get_round_robin();
        broadcast
            .append(&pkt, &mut io, scratch, &mut compressor)
            .unwrap();
    }

    // update the tick
    global.tick += 1;
}
