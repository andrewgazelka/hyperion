use std::time::Instant;

use evenio::prelude::*;
use tracing::{instrument, trace};

use crate::{
    components::KeepAlive,
    event::{Gametick, KickPlayer},
    global::Global,
    net::{Compose, StreamId},
    system::player_join_world::send_keep_alive,
};

#[instrument(skip_all, level = "trace")]
pub fn keep_alive(
    _: Receiver<Gametick>,
    global: Single<&Global>,
    mut fetcher: Fetcher<(EntityId, &mut KeepAlive, &mut StreamId)>,
    s: Sender<KickPlayer>,
    compose: Compose,
) {
    fetcher.iter_mut().for_each(|(id, keep_alive, packets)| {
        let Some(sent) = &mut keep_alive.last_sent else {
            keep_alive.last_sent = Some(Instant::now());
            return;
        };

        // if we haven't sent a keep alive packet in 5 seconds, and a keep alive hasn't already
        // been sent and hasn't been responded to, send one
        let elapsed = sent.elapsed();

        if elapsed > global.keep_alive_timeout {
            s.send_to(id, KickPlayer {
                reason: "keep alive timeout".into(),
            });
            return;
        }

        if !keep_alive.unresponded && elapsed.as_secs() >= 5 {
            *sent = Instant::now();

            send_keep_alive(*packets, &compose).unwrap();

            trace!("keep alive");
        }
    });
}
