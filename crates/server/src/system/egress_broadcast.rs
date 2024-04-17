use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::instrument;

use crate::{
    events::Egress,
    net::{Fd, Server},
    singleton::{broadcast::BroadcastBuf, buffer_allocator::BufferAllocator},
};

#[instrument(skip_all, level = "trace")]
pub fn egress_broadcast(
    _: Receiver<Egress>,
    bufs: Single<&BufferAllocator>,
    mut server: Single<&mut Server>,
    fds: Fetcher<&Fd>,
    mut broadcast: Single<&mut BroadcastBuf>,
) {
    // let mut buf = bufs.obtain().unwrap();
    // buf.clear();

    broadcast.drain(|bytes| {
        // buf.try_extend_from_slice(&bytes).unwrap();
    });

    // println!("broadcast buf len: {}", buf.len());
    //
    // server.broadcast(&buf, fds.iter().copied());
}
