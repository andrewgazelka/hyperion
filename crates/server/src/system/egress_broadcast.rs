use evenio::{event::Receiver, fetch::Single};
use tracing::instrument;

use crate::{singleton::broadcast::BroadcastBuf, Egress};

#[instrument(skip_all, level = "trace")]
pub fn egress_broadcast(
    _: Receiver<Egress>,
    //    connections: Fetcher<&Connection>,
    _broadcast: Single<&mut BroadcastBuf>,
) {
    //    broadcast.par_drain(|buf| {
    //        for connection in &connections {
    //            trace!("about to broadcast bytes {:?}", buf.len());
    //            let _ = connection.send(buf.clone());
    //        }
    //    });
}
