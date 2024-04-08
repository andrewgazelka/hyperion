use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::{instrument, trace};

use crate::{net::Connection, singleton::encoder::Broadcast, Egress};

#[instrument(skip_all, level = "trace")]
pub fn egress_broadcast(
    _: Receiver<Egress>,
    connections: Fetcher<&Connection>,
    broadcast: Single<&mut Broadcast>,
) {
    let broadcast = broadcast.0;

    broadcast.par_drain(|buf| {
        for connection in &connections {
            trace!("about to broadcast bytes {:?}", buf.len());
            let _ = connection.send(buf.clone());
        }
    });
}
