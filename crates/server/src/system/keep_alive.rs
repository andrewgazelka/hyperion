use std::time::Instant;

use evenio::{prelude::*};
use tracing::{debug, instrument, trace};

use crate::{
    global::Global, net::Encoder, system::player_join_world::send_keep_alive, Gametick, Player,
};

#[instrument(skip_all, level = "trace")]
pub fn keep_alive(
    _: Receiver<Gametick>,
    global: Single<&Global>,
    mut fetcher: Fetcher<(&mut Player, &mut Encoder)>,
) {
    fetcher.iter_mut().for_each(|(player, encoder)| {
        // if we haven't sent a keep alive packet in 5 seconds, and a keep alive hasn't already
        // been sent and hasn't been responded to, send one
        if !player.unresponded_keep_alive && player.last_keep_alive_sent.elapsed().as_secs() >= 5 {
            player.last_keep_alive_sent = Instant::now();
            // todo: handle and disconnect
            let name = &player.name;
            debug!("sending keep alive to {name}");

            send_keep_alive(encoder, &global).unwrap();

            trace!("keep alive");
        }
    });
}
