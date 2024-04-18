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
    net::{Broadcast, Fd, IoBuf, Packets, RefreshItems, Server, ServerDef},
};

#[instrument(skip_all, level = "trace")]
pub fn egress(
    _: Receiver<Egress>,
    mut server: Single<&mut Server>,
    players: Fetcher<(&Packets, &Fd)>,
    broadcast: Single<&Broadcast>,
    encoders: Fetcher<(&IoBuf, &Fd, &LoginState)>,
    mut global: Single<&mut Global>,
) {
    let items = encoders.iter().map(|(encoder, fd, state)| RefreshItems {
        local: encoder.buf(),
        fd: *fd,
        broadcast: *state == LoginState::Play,
    });

    server.write_all(&mut global, &broadcast_buf, items);

    server.submit_events();
}
