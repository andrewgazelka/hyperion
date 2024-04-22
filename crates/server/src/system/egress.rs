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
    // todo: idk how inefficient this is
    for (pkts, fd, login_state) in &mut players {
        for &mut broadcast_write in broadcast.get_write() {
            if *login_state == LoginState::Play {
                pkts.get_write().push_back(broadcast_write);
            }
        }
    }

    let local_items =
        players
            .iter_mut()
            .filter(|(pkts, ..)| pkts.can_send())
            .map(|(pkts, fd, login_state)| RefreshItems {
                write: pkts.get_write(),
                fd: *fd,
                broadcast: *login_state == LoginState::Play,
            });

    let mut event = r.event;
    let server = &mut *event.server;

    server.write_all(&mut global, local_items);
    server.submit_events();

    // now clear
    broadcast.clear();

    for (pkts, ..) in &mut players {
        if pkts.can_send() {
            pkts.set_sending();
            pkts.clear();
        }
    }
}
