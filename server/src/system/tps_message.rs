use evenio::prelude::*;
use tracing::instrument;

use crate::{bytes_to_mb, Player, StatsEvent};

#[instrument(skip_all, name = "tps_message")]
pub fn tps_message(r: Receiver<StatsEvent>, mut players: Fetcher<&mut Player>) {
    let StatsEvent {
        ms_per_tick_mean,
        ms_per_tick_std_dev,
        resident,
        ..
    } = r.event;

    // let allocated = bytes_to_mb(*allocated);
    let resident = bytes_to_mb(*resident);

    // make sexy with stddev & mean symbol
    let message = format!("µ={ms_per_tick_mean:.2}, σ={ms_per_tick_std_dev:.2}, {resident:.2}MiB");

    players.iter_mut().for_each(|player| {
        // todo: handle error
        let _ = player.packets.writer.send_chat_message(&message);
    });
}
