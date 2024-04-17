use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::instrument;

use crate::{
    events::Egress,
    global::Global,
    net::{Fd, LocalEncoder, Server, ServerDef},
    singleton::{
        broadcast::BroadcastBuf,
        buffer_allocator::{BufRef, BufferAllocator},
    },
};

#[instrument(skip_all, level = "trace")]
pub fn egress_broadcast(
    _: Receiver<Egress>,
    bufs: Single<&BufferAllocator>,
    mut server: Single<&mut Server>,
    fds: Fetcher<&Fd>,
    global: Single<&Global>,
    mut broadcast: Single<&mut BroadcastBuf>,

    encoders: Fetcher<&mut LocalEncoder>,
) {
    let mut buf = bufs.obtain().unwrap();
    buf.clear();

    broadcast.drain(|bytes| {
        buf.try_extend_from_slice(&bytes).unwrap();
    });

    // works(&buf, encoders, global);
    // does_not_work(&buf, server, &fds);
}

fn works(buf: &[u8], encoders: Fetcher<&mut LocalEncoder>, global: Single<&Global>) {
    for encoder in encoders {
        encoder.append_raw(buf, &global).unwrap();
    }
}

fn does_not_work(buf: &BufRef, mut server: Single<&mut Server>, fds: &Fetcher<&Fd>) {
    server.broadcast(buf, fds.iter().copied());
}
