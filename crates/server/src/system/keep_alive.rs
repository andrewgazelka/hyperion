use std::time::Instant;

use evenio::prelude::*;
use tracing::{instrument, trace};

use crate::{
    components::KeepAlive,
    events::{Gametick, KickPlayer},
    global::Global,
    net::LocalEncoder,
    system::player_join_world::send_keep_alive,
};

#[instrument(skip_all, level = "trace")]
pub fn keep_alive(
    _: Receiver<Gametick>,
    global: Single<&Global>,
    mut fetcher: Fetcher<(EntityId, &mut KeepAlive, &mut LocalEncoder)>,
    mut s: Sender<KickPlayer>,
) {
    fetcher.iter_mut().for_each(|(id, keep_alive, encoder)| {
        // if we haven't sent a keep alive packet in 5 seconds, and a keep alive hasn't already
        // been sent and hasn't been responded to, send one
        let elapsed = keep_alive.last_sent.elapsed();

        if elapsed > global.keep_alive_timeout {
            s.send(KickPlayer {
                target: id,
                reason: "keep alive timeout".into(),
            });
            return;
        }

        if !keep_alive.unresponded && elapsed.as_secs() >= 5 {
            keep_alive.last_sent = Instant::now();

            // todo: handle and disconnect
            send_keep_alive(encoder, &global).unwrap();

            trace!("keep alive");
        }
    });
}
