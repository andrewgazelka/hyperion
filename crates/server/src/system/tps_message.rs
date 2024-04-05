use evenio::prelude::*;
use tracing::instrument;

use crate::{
    bytes_to_mb, system::player_join_world::encode_chat_message, FullEntityPose, Player, StatsEvent,
};

#[instrument(skip_all, name = "tps_message")]
pub fn tps_message(r: Receiver<StatsEvent>, mut players: Fetcher<(&mut Player, &FullEntityPose)>) {
    let StatsEvent {
        ms_per_tick_mean_1s,
        ms_per_tick_mean_5s,
        resident,
        ..
    } = r.event;

    // // let allocated = bytes_to_mb(*allocated);
    // let resident = bytes_to_mb(*resident);
    //
    // players.iter_mut().for_each(|(player, _)| {
    //     // make sexy with stddev & mean symbol
    //     let ping = player.ping.as_secs_f32() * 1000.0;
    //     let speed_mib = f64::from(player.packets.writer.speed_mib_per_second()) / 1024.0 /
    // 1024.0;     let message = format!(
    //         "µms {ms_per_tick_mean_1s:.2} {ms_per_tick_mean_5s:.2}, {resident:.2}MiB, \
    //          {speed_mib:.2}MiB/s, {ping:.2}ms"
    //     );
    //
    //     // todo: handle error
    //     // encode_chat_message(player.packets.writer.en)
    //     // let _ = player.packets.writersend_chat_message(&message);
    // });
}
