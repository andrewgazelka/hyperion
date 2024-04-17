use bytes::Bytes;
use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::{info, instrument};

use crate::{
    components::LoginState,
    events::Egress,
    global::Global,
    net::{Fd, LocalEncoder, RefreshItem, Server, ServerDef},
    singleton::{
        broadcast::BroadcastBuf,
        buffer_allocator::{BufRef, BufferAllocator},
    },
};

#[instrument(skip_all, level = "trace")]
pub fn egress(
    _: Receiver<Egress>,
    bufs: Single<&BufferAllocator>,
    mut server: Single<&mut Server>,
    encoders: Fetcher<(&LocalEncoder, &Fd, &LoginState)>,
    mut global: Single<&mut Global>,
    mut broadcast: Single<&mut BroadcastBuf>,
) {
    let mut broadcast_buf = bufs.obtain().unwrap();

    println!("broadcast buf: {broadcast_buf:?}");

    // print player bufs
    for (encoder, fd, state) in encoders.iter() {
        println!("encoder: {encoder:?}");

        if encoder.buf().len() < 1024 {
            let bytes: Bytes = encoder.buf().iter().copied().collect();

            println!("encoder before: {bytes:?}");
            continue;
        }
    }

    broadcast_buf.clear();

    broadcast.drain(|bytes| {
        broadcast_buf.try_extend_from_slice(&bytes).unwrap();
    });

    let items = encoders.iter().map(|(encoder, fd, state)| RefreshItem {
        local: encoder.buf(),
        fd: *fd,
        broadcast: *state == LoginState::Play,
    });

    server.write_all(&mut global, &broadcast_buf, items);

    server.submit_events();
}
