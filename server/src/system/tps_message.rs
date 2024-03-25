use evenio::prelude::*;
use tracing::instrument;

use crate::{Player, TpsEvent};

#[instrument(skip_all, name = "tps_message")]
pub fn tps_message(r: Receiver<TpsEvent>, mut players: Fetcher<&mut Player>) {
    let ms_per_tick = r.event.ms_per_tick;

    // with 4 zeroes
    // lead 2 zeroes
    let message = format!("MSPT: {ms_per_tick:07.4}");

    players.iter_mut().for_each(|player| {
        // todo: handle error
        let _ = player.packets.writer.send_chat_message(&message);
    });
}
