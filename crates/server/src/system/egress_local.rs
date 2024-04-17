use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::instrument;

use crate::{
    events::Egress,
    global::Global,
    net::{LocalEncoder, Server, ServerDef},
};

#[instrument(skip_all, level = "trace")]
pub fn egress_local(
    _: Receiver<Egress>,
    _connections: Fetcher<&mut LocalEncoder>,
    _global: Single<&Global>,
    mut server: Single<&mut Server>,
) {
    server.submit_events();
    //    let compression = global.0.shared.compression_level;
    //
    //    connections
    //        .par_iter_mut()
    //        .for_each(|(connection, encoder)| {
    //            let bytes = encoder.take(compression);
    //            if bytes.is_empty() {
    //                return;
    //            }
    //            trace!("about to send bytes {:?}", bytes.len());
    //            let _ = connection.send(bytes);
    //        });
}
