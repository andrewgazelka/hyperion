use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::text::IntoText;

use crate::{
    singleton::encoder::{Broadcast, PacketMetadata},
    StatsEvent,
};

#[instrument(skip_all, level = "trace")]
pub fn stats_message(r: Receiver<StatsEvent>, encoder: Single<&mut Broadcast>) {
    let StatsEvent {
        ms_per_tick_mean_1s,
        ms_per_tick_mean_5s,
        ..
    } = r.event;

    let message = format!("ms {ms_per_tick_mean_1s:05.2} {ms_per_tick_mean_5s:05.2}");
    let packet = valence_protocol::packets::play::OverlayMessageS2c {
        action_bar_text: message.into_cow_text(),
    };

    let encoder = encoder.0;

    encoder
        .get_round_robin()
        .append_packet(&packet, PacketMetadata::DROPPABLE)
        .unwrap();
}
