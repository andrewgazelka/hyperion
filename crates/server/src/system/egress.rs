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
    net::{
        encoder::PacketWriteInfo, Broadcast, Fd, GlobalPacketWriteInfo, Packets, ServerDef,
        WriteItem,
    },
};

pub fn egress(
    r: ReceiverMut<Egress>,
    players: Fetcher<(&SyncUnsafeCell<Packets>, &Fd, &LoginState)>,
    mut broadcast: Single<&mut Broadcast>,
) {
    let egress_span = tracing::span!(tracing::Level::TRACE, "egress");
    let _enter = egress_span.enter();

    let broadcast = &mut *broadcast;

    let combined = tracing::span!(tracing::Level::TRACE, "broadcast-combine").in_scope(|| {
        let total_len: usize = broadcast.iter().map(|x| x.data.len()).sum();

        let mut combined = Vec::with_capacity(total_len);

        for data in broadcast.iter_mut().map(|x| x.data.as_mut()) {
            combined.append(data);
        }

        combined
    });

    let combined_len = combined.len() as u32;

    broadcast
        .get_all_mut()
        .par_iter_mut()
        .for_each(|broadcast| {
            let ptr = broadcast.buffer.append(combined.as_slice());
            broadcast.local_to_write = PacketWriteInfo {
                start_ptr: ptr,
                len: combined_len,
            };
        });

    let mut event = r.event;
    let servers = &mut *event.server;
    
    rayon::broadcast(|_| {
        let server = servers.get_local_raw();
        let server = unsafe { &mut *server.get() };
        let send_span = tracing::span!(parent: &egress_span, tracing::Level::TRACE, "send");
        let _enter = send_span.enter();

        let broadcast = broadcast.get_local_raw();
        let broadcast = unsafe { &mut *broadcast.get() };

        let send = broadcast.local_to_write;

        let global_send = (send.len != 0).then_some(GlobalPacketWriteInfo {
            start_ptr: send.start_ptr,
            len: send.len,
            buffer_idx: broadcast.buffer.index(),
        });
        
        let global_send_count = usize::from(global_send.is_some());

        for &id in &server.fd_ids {
            // trace!("sending to {id:?}");
            let Ok((pkts, fd, login)) = players.get(id) else {
                warn!("no player found for id {id:?}");
                return;
            };

            let pkts = unsafe { &mut *pkts.get() };
            
            let extra = if login.is_play() {
                global_send_count
            } else {
                0
            };

            // trace!("is play? {}", login.is_play());

            let can_send = pkts.can_send(extra);

            if !can_send {
                // trace!("cannot send");
                continue;
            }

            // trace!("can send");

            pkts.prepare_for_send(extra);
            let buffer_idx = pkts.index();

            let global = login.is_play().then_some(()).and(global_send);

            let write_item = WriteItem {
                local: pkts.get_write_mut(),
                global,
                fd: *fd,
                buffer_idx,
            };

            // trace!("writing to {fd:?}");

            server.inner.write(write_item);
        }
    });

    tracing::span!(tracing::Level::TRACE, "submit-events",).in_scope(|| {
        rayon::broadcast(|_| {
            let server = servers.get_local_raw();
            let server = unsafe { &mut *server.get() };
            server.submit_events();
        });
    });
}
