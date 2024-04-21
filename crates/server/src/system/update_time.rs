use evenio::prelude::*;
use tracing::instrument;

use crate::{
    events::Gametick,
    global::Global,
    net::{IoBuf},
};
use crate::net::Broadcast;

#[instrument(skip_all, level = "trace")]
pub fn update_time(
    _: ReceiverMut<Gametick>,
    mut broadcast: Single<&mut Broadcast>,
    mut global: Single<&mut Global>,
    mut io: Single<&mut IoBuf>,
) {
    let tick = global.tick;
    let time_of_day = tick % 24000;

    // Only sync with the client every 5 seconds
    if tick % (20 * 5) == 0 {
        let pkt = valence_protocol::packets::play::WorldTimeUpdateS2c {
            world_age: tick,
            time_of_day,
        };

        broadcast.append(&pkt, &mut io).unwrap();
    }

    // update the tick
    global.tick += 1;
}
