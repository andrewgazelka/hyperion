use std::cell::SyncUnsafeCell;

use evenio::{
    event::ReceiverMut,
    fetch::{Fetcher, Single},
};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::{instrument, log::warn, trace};

use crate::{
    components::LoginState,
    event::Egress,
    global::Global,
    net::{Broadcast, Fd, GlobalPacketWriteInfo, Packets, ServerDef, WriteItem},
};

#[instrument(skip_all, level = "trace")]
pub fn egress(
    r: ReceiverMut<Egress>,
    mut players: Fetcher<(&SyncUnsafeCell<Packets>, &Fd, &LoginState)>,
    mut broadcast: Single<&mut Broadcast>,
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

    let mut event = r.event;
    let servers = &mut *event.server;

    servers.get_all_mut().par_iter_mut().for_each(|server| {
        for &id in &server.fd_ids {
            let Ok((pkts, fd, login)) = players.get(id) else {
                warn!("no player found for id {id:?}");
                return;
            };

            let pkts = unsafe { &mut *pkts.get() };

            let extra = if login.is_play() {
                global_send.len()
            } else {
                0
            };
            let can_send = pkts.can_send(extra);

            if !can_send {
                continue;
            }

            pkts.prepare_for_send(extra);
            let buffer_idx = pkts.index();

            let global = login.is_play().then_some(&global_send);

            let write_item = WriteItem {
                local: pkts.get_write_mut(),
                global,
                fd: *fd,
                buffer_idx,
            };

            trace!("writing to {fd:?}");

            server.inner.write(write_item);
        }
    });

    tracing::span!(tracing::Level::TRACE, "submit-events",).in_scope(|| {
        servers.get_all_mut().iter_mut().for_each(|server| {
            server.submit_events();
        });
    });

    // now clear
    tracing::span!(tracing::Level::TRACE, "clear-broadcast").in_scope(|| {
        broadcast.clear();
    });
}
