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
    for (pkts, _, login_state) in &mut players {
        for &mut broadcast_write in broadcast.get_write_mut() {
            if *login_state == LoginState::Play {
                pkts.get_write_mut().push_back(broadcast_write);
            }
        }
    }

    let local_items =
        players
            .iter_mut()
            .filter(|(pkts, ..)| pkts.can_send())
            .map(|(pkts, fd, _)| {
                pkts.prepare_for_send(); // todo: should we not do this in a map for clarity?
                RefreshItems {
                    write: pkts.get_write_mut(),
                    fd: *fd,
                }
            });

    let mut event = r.event;
    let server = &mut *event.server;

    server.write_all(&mut global, local_items);
    server.submit_events();

    // now clear
    broadcast.clear();
}
