use evenio::prelude::*;
use tracing::instrument;

use crate::{
    events::Gametick,
    global::Global,
    net::{Broadcast, Compressor, IoBufs},
};

#[instrument(skip_all, level = "trace")]
pub fn update_time(
    gametick: ReceiverMut<Gametick>,
    mut broadcast: Single<&mut Broadcast>,
    mut global: Single<&mut Global>,
    mut io: Single<&mut IoBufs>,
    mut compressor: Single<&mut Compressor>,
) {
    let mut gametick = gametick.event;

    let gametick = &mut *gametick;

    let scratch = &mut gametick.scratch;

    let tick = global.tick;
    let time_of_day = tick % 24000;

    // Only sync with the client every 5 seconds
    if tick % (20 * 5) == 0 {
        let pkt = valence_protocol::packets::play::WorldTimeUpdateS2c {
            world_age: tick,
            time_of_day,
        };

        let scratch = scratch.one();
        broadcast
            .append(&pkt, io.one(), scratch, &mut compressor.one())
            .unwrap();
    }

    // update the tick
    global.tick += 1;
}
