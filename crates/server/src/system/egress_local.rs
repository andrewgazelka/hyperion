use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::instrument;

use crate::{
    events::Egress,
    global::Global,
    net::{Fd, LocalEncoder, Server, ServerDef},
};

#[instrument(skip_all, level = "trace")]
pub fn egress_local(
    _: Receiver<Egress>,
    mut global: Single<&mut Global>,
    encoders: Fetcher<(&LocalEncoder, &Fd)>,
    mut server: Single<&mut Server>,
) {
    let encoders = encoders.iter().map(|(encoder, fd)| (encoder.buf(), *fd));
    server.write(&mut global, encoders);
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
