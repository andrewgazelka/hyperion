use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::{info, instrument};

use crate::{
    components::LoginState,
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
    fds: Fetcher<(&Fd, &LoginState)>,
    global: Single<&Global>,
    mut broadcast: Single<&mut BroadcastBuf>,
) {
    let mut buf = bufs.obtain().unwrap();
    buf.clear();

    broadcast.drain(|bytes| {
        buf.try_extend_from_slice(&bytes).unwrap();
    });
    
    let fds = fds
        .iter()
        .filter(|(fd, state)| **state == LoginState::Play)
        .map(|(fd, _)| fd)
        .copied();

    server.broadcast(&buf, fds);
}
