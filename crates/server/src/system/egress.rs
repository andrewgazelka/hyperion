use evenio::{
    event::ReceiverMut,
    fetch::{Fetcher, Single},
};
use tracing::instrument;

use crate::{
    components::LoginState,
    events::Egress,
    global::Global,
    net::{Broadcast, Fd, Packets, RefreshItems, ServerDef},
};

#[instrument(skip_all, level = "trace")]
pub fn egress(
    r: ReceiverMut<Egress>,
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

    let mut event = r.event;
    let server = &mut *event.server;

    server.write_all(&mut global, broadcast.to_write(), local_items);

    server.submit_events();

    // now clear
    broadcast.clear();

    for (pkts, ..) in &mut players {
        pkts.clear();
    }
}
