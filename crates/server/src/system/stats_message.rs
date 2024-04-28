use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::text::IntoText;

use crate::{
    event::Stats,
    net::{Broadcast, Compose},
};

#[instrument(skip_all, level = "trace")]
pub fn stats_message(r: ReceiverMut<Stats>, broadcast: Single<&Broadcast>, compose: Compose) {
    let event = r.event;

    let ms_per_tick_mean_1s = event.ms_per_tick_mean_1s;
    let ms_per_tick_mean_5s = event.ms_per_tick_mean_5s;

    let message = format!("ms {ms_per_tick_mean_1s:05.2} {ms_per_tick_mean_5s:05.2}");
    let packet = valence_protocol::packets::play::OverlayMessageS2c {
        action_bar_text: message.into_cow_text(),
    };

    broadcast.append(&packet, &compose).unwrap();
}
