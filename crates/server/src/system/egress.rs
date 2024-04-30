use std::ops::DerefMut;

use evenio::{
    event::ReceiverMut,
    fetch::{Fetcher, Single},
};
use tracing::{instrument, trace};

use crate::{
    components::LoginState,
    event::Egress,
    global::Global,
    net::{Broadcast, Fd, GlobalPacketWriteInfo, Packets, RefreshItems, ServerDef},
};

#[instrument(skip_all, level = "trace")]
pub fn egress(
    r: ReceiverMut<Egress>,
    mut players: Fetcher<(&mut Packets, &Fd, &LoginState)>,
    mut broadcast: Single<&mut Broadcast>,
    mut global: Single<&mut Global>,
) {
    let broadcast = &mut *broadcast;

    let mut global_send = Vec::new();
    // todo: idk how inefficient this is
    tracing::span!(tracing::Level::TRACE, "extend-from-broadcast").in_scope(|| {
        for local in broadcast.iter_mut() {
            let index = local.buffer.index();
            for elem in &local.local_to_write {
                global_send.push(GlobalPacketWriteInfo {
                    start_ptr: elem.start_ptr,
                    len: elem.len,
                    buffer_idx: index,
                });
            }
        }
    });

    trace!("global send size: {}", global_send.len());

    let mut total_items = 0;

    let local_items =
        tracing::span!(tracing::Level::TRACE, "generate-refresh-items").in_scope(|| {
            players
                .iter_mut()
                .filter(|(pkts, _, login)| {
                    let extra = if login.is_play() {
                        global_send.len()
                    } else {
                        0
                    };
                    pkts.can_send(extra)
                })
                .map(|(pkts, fd, login)| {
                    let extra = if login.is_play() {
                        global_send.len()
                    } else {
                        0
                    };
                    total_items += pkts.prepare_for_send(extra); // todo: should we not do this in a map for clarity?
                    let buffer_idx = pkts.index();

                    let global = login.is_play().then_some(&global_send);

                    RefreshItems {
                        local: pkts.get_write_mut(),
                        global,
                        fd: *fd,
                        buffer_idx,
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
