use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::instrument;

use crate::{global::Global, net::Encoder, Egress};

#[instrument(skip_all, level = "trace")]
pub fn egress_local(
    _: Receiver<Egress>,
    connections: Fetcher<&mut Encoder>,
    global: Single<&Global>,
) {
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
