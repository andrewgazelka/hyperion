use evenio::prelude::*;
use tracing::instrument;

use crate::{Gametick, Player};

#[instrument(skip_all, level = "trace")]
pub fn clean_up_io(
    _r: Receiver<Gametick>,
    mut io_entities: Fetcher<(EntityId, &mut Player)>,

    mut s: Sender<Despawn>,
) {
    for (id, player) in &mut io_entities {
        if player.packets.writer.is_closed() {
            s.send(Despawn(id));
        }
    }
}
