use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::text::IntoText;

use crate::{
    event::Stats,
    global::Global,
    net::{Broadcast, Compose},
};

#[instrument(skip_all, level = "trace")]
pub fn stats_message(
    r: ReceiverMut<Stats>,
    broadcast: Single<&Broadcast>,
    compose: Compose,
    global: Single<&Global>,
) {
    let event = r.event;

    let ms_per_tick_mean_1s = event.ms_per_tick_mean_1s;
    let ms_per_tick_mean_5s = event.ms_per_tick_mean_5s;

    let mspt = format!("ms {ms_per_tick_mean_1s:05.2} {ms_per_tick_mean_5s:05.2}");
    let mspt = mspt.into_cow_text();

    let player_count = global
        .shared
        .player_count
        .load(std::sync::atomic::Ordering::Relaxed);
    let player_count = format!("{player_count} player online");
    let player_count = player_count.into_cow_text();

    // header footer
    let pkt = valence_protocol::packets::play::PlayerListHeaderS2c {
        header: mspt,
        footer: player_count,
    };

    broadcast.append(&pkt, &compose).unwrap();
}
