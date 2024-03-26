use evenio::prelude::*;
use tracing::instrument;

use crate::{bytes_to_mb, FullEntityPose, Player, StatsEvent};

#[instrument(skip_all, name = "tps_message")]
pub fn tps_message(
    r: Receiver<StatsEvent>,
    mut players: Fetcher<(&mut Player, &FullEntityPose)>
) {
    let StatsEvent {
        ms_per_tick_mean_1s,
        ms_per_tick_mean_5s,
        resident,
        ..
    } = r.event;

    // let allocated = bytes_to_mb(*allocated);
    let resident = bytes_to_mb(*resident);

    players.iter_mut().for_each(|(player, _)| {
        // make sexy with stddev & mean symbol
        let message = format!("µms {ms_per_tick_mean_1s:.2} {ms_per_tick_mean_5s:.2}, {resident:.2}MiB");

        // todo: handle error
        let _ = player.packets.writer.send_chat_message(&message);
    });
}
