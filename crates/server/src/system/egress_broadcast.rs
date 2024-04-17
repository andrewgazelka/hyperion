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
    // mut encoders: Fetcher<(&mut LocalEncoder, &LoginState)>,
) {
    // info!("broadcasting");
    let mut buf = bufs.obtain().unwrap();
    buf.clear();

    broadcast.drain(|bytes| {
        buf.try_extend_from_slice(&bytes).unwrap();
    });

    // let encoders = encoders
    //     .iter_mut()
    //     .filter(|(_, state)| **state == LoginState::Play)
    //     .map(|(encoder, _)| encoder);

    let fds = fds
        .iter()
        .filter(|(fd, state)| **state == LoginState::Play)
        .map(|(fd, _)| fd)
        .copied();

    // works(&full_buf, encoders, global);
    does_not_work(&buf, server, fds);
}

fn works<'a>(
    buf: &[u8],
    encoders: impl Iterator<Item = &'a mut LocalEncoder>,
    global: Single<&Global>,
) {
    for encoder in encoders {
        encoder.append_raw(buf, &global).unwrap();
    }
}

fn does_not_work(buf: &BufRef, mut server: Single<&mut Server>, fds: impl Iterator<Item = Fd>) {
    server.broadcast(buf, fds);
}
