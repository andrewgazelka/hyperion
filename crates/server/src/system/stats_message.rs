use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::text::IntoText;

use crate::{
    events::StatsEvent,
    net::{Broadcast, Compressor, IoBufs},
};

#[instrument(skip_all, level = "trace")]
pub fn stats_message(
    r: ReceiverMut<StatsEvent>,
    mut broadcast: Single<&mut Broadcast>,
    mut compressor: Single<&mut Compressor>,
    mut io: Single<&mut IoBufs>,
) {
    let mut event = r.event;

    let ms_per_tick_mean_1s = event.ms_per_tick_mean_1s;
    let ms_per_tick_mean_5s = event.ms_per_tick_mean_5s;

    let scratch = &mut *event.scratch;

    let message = format!("ms {ms_per_tick_mean_1s:05.2} {ms_per_tick_mean_5s:05.2}");
    let packet = valence_protocol::packets::play::OverlayMessageS2c {
        action_bar_text: message.into_cow_text(),
    };

    broadcast
        .append(&packet, io.one(), scratch, &mut compressor.one())
        .unwrap();
}
