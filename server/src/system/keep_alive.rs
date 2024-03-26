use std::time::Instant;

use evenio::{prelude::*, rayon::prelude::*};
use tracing::{debug, instrument};

use crate::{Gametick, Player};

#[instrument(skip_all)]
pub fn keep_alive(_: Receiver<Gametick>, mut fetcher: Fetcher<&mut Player>) {
    fetcher.par_iter_mut().for_each(|player| {
        // if we haven't sent a keep alive packet in 5 seconds, send one
        if player.last_keep_alive_sent.elapsed().as_secs() >= 5 {
            player.last_keep_alive_sent = Instant::now();
            // todo: handle and disconnect
            let name = &player.name;
            debug!("sending keep alive to {name}");
            let _ = player.packets.writer.send_keep_alive();
        }
    });
}
