use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::{instrument, trace};

use crate::{singleton::broadcast::BroadcastBuf, Egress};

#[instrument(skip_all, level = "trace")]
pub fn egress_broadcast(
    _: Receiver<Egress>,
    //    connections: Fetcher<&Connection>,
    mut broadcast: Single<&mut BroadcastBuf>,
) {
    //    broadcast.par_drain(|buf| {
    //        for connection in &connections {
    //            trace!("about to broadcast bytes {:?}", buf.len());
    //            let _ = connection.send(buf.clone());
    //        }
    //    });
}
