use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::text::IntoText;

use crate::{singleton::broadcast::Broadcast, StatsEvent};

#[instrument(skip_all, level = "trace")]
pub fn stats_message(r: Receiver<StatsEvent>, mut broadcast: Single<&mut Broadcast>) {
    let StatsEvent {
        ms_per_tick_mean_1s,
        ms_per_tick_mean_5s,
        ..
    } = r.event;

    let message = format!("ms {ms_per_tick_mean_1s:05.2} {ms_per_tick_mean_5s:05.2}");
    let packet = valence_protocol::packets::play::OverlayMessageS2c {
        action_bar_text: message.into_cow_text(),
    };

    broadcast.get_round_robin().append_packet(&packet).unwrap();
}
