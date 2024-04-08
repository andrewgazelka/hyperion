use bytes::Bytes;
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
        // TODO: Avoid taking packet_data so that the capacity can be reused
        let packet_data = Bytes::from(core::mem::take(&mut buf.packet_data));

        for connection in &connections {
            trace!("about to broadcast bytes {:?}", packet_data.len());
            let _ = connection.send(packet_data.clone());
        }

        // RNG.set(Some(rng));
        buf.clear_packets();
    });
}
