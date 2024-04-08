use evenio::{event::Receiver, fetch::Fetcher};
use rayon::prelude::*;
use tracing::{instrument, trace};

use crate::{
    net::{Connection, Encoder},
    Egress,
};

#[instrument(skip_all, level = "trace")]
pub fn egress_local(_: Receiver<Egress>, mut connections: Fetcher<(&Connection, &mut Encoder)>) {
    connections
        .par_iter_mut()
        .for_each(|(connection, encoder)| {
            let bytes = encoder.take();
            trace!("about to send bytes {:?}", bytes.len());
            let _ = connection.send(bytes);
        });
}
