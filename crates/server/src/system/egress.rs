use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::instrument;

use crate::{
    components::LoginState,
    events::Egress,
    global::Global,
    net::{Broadcast, Fd, Packets, RefreshItems, Server, ServerDef},
};

#[instrument(skip_all, level = "trace")]
pub fn egress(
    _: Receiver<Egress>,
    mut server: Single<&mut Server>,
    mut players: Fetcher<(&mut Packets, &Fd, &LoginState)>,
    mut broadcast: Single<&mut Broadcast>,
    mut global: Single<&mut Global>,
) {
    let local_items = players
        .iter_mut()
        .map(|(pkts, fd, login_state)| RefreshItems {
            write: pkts.to_write(),
            fd: *fd,
            broadcast: *login_state == LoginState::Play,
        });

    server.write_all(&mut global, broadcast.to_write(), local_items);

    server.submit_events();

    // now clear
    broadcast.clear();

    for (pkts, ..) in &mut players {
        pkts.clear();
    }
}
