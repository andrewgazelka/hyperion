
use evenio::{
    event::ReceiverMut,
    fetch::{Fetcher, Single},
};
use tracing::instrument;

use crate::{
    components::LoginState,
    event::Egress,
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
    let broadcast = &mut *broadcast;
    // todo: idk how inefficient this is
    tracing::span!(tracing::Level::TRACE, "extend-from-broadcast").in_scope(|| {
        for (pkts, _, login_state) in &mut players {
            if *login_state == LoginState::Play {
                pkts.extend(broadcast);
            }
        }
    });

    let mut total_items = 0;

    let local_items =
        tracing::span!(tracing::Level::TRACE, "generate-refresh-items").in_scope(|| {
            players
                .iter_mut()
                .filter(|(pkts, ..)| pkts.can_send())
                .map(|(pkts, fd, _)| {
                    total_items += pkts.prepare_for_send(); // todo: should we not do this in a map for clarity?
                    RefreshItems {
                        write: pkts.get_write_mut(),
                        fd: *fd,
                    }
                })
        });

    let mut event = r.event;
    let server = &mut *event.server;

    server.write_all(&mut global, local_items);

    let player_count = players.iter_mut().len();
    let per_player = total_items as f64 / player_count as f64;

    tracing::span!(
        tracing::Level::TRACE,
        "submit-events",
        total_items,
        per_player
    )
    .in_scope(|| {
        server.submit_events();
    });

    // now clear
    tracing::span!(tracing::Level::TRACE, "clear-broadcast").in_scope(|| {
        broadcast.clear();
    });
}
