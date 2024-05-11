use evenio::{
    event::ReceiverMut,
    fetch::{Fetcher, Single},
};
use tracing::{instrument, log::warn};

use crate::{
    components::LoginState,
    event::Egress,
    net::{encoder::DataWriteInfo, Broadcast, Fd, ServerDef, WriteItem},
};

#[instrument(skip_all, level = "trace")]
pub fn egress(
    r: ReceiverMut<Egress>,
    mut players: Fetcher<(&Fd, &LoginState)>,
    broadcast: Single<&mut Broadcast>,
) {
    let broadcast = broadcast.0;

//    let combined = tracing::span!(tracing::Level::TRACE, "broadcast-combine").in_scope(|| {
//        let total_len: usize = broadcast.packets().iter().map(|x| x.data.len()).sum();
//
//        let mut combined = Vec::with_capacity(total_len);
//
//        for data in broadcast.packets_mut().iter_mut().map(|x| x.data.as_mut()) {
//            combined.append(data);
//        }
//
//        combined
//    });

    // let broadcast_len = combined.len() as u32;

//    let ptr = broadcast.buffer.append(combined.as_slice());
//    broadcast.local_to_write = DataWriteInfo {
//        start_ptr: ptr,
//        len: broadcast_len,
//    };
//
//    let broadcast_index = broadcast.buffer.index();
//
//    let mut event = r.event;
//    let server = &mut *event.server;
//
//    tracing::span!(tracing::Level::TRACE, "send",).in_scope(|| {
//        for (pkts, fd, login) in &mut players {
//            if !pkts.can_send() {
//                continue;
//            }
//
//            let index = pkts.index();
//
//            for elem in &pkts.local_to_write {
//                if elem.len == 0 {
//                    continue;
//                }
//
//                let write_item = WriteItem {
//                    info: elem,
//                    buffer_idx: index,
//                    fd: *fd,
//                };
//
//                pkts.number_sending += 1;
//
//                server.inner.write(write_item);
//            }
//
//            pkts.elems_mut().clear();
//
//            // no broadcasting if we are not in play state
//            if *login != LoginState::Play {
//                continue;
//            }
//
//            // todo: append broadcast even if cannot send and have packet prios and stuff
//            if broadcast_len != 0 {
//                pkts.number_sending += 1;
//                server.inner.write(WriteItem {
//                    info: &broadcast.local_to_write,
//                    buffer_idx: broadcast_index,
//                    fd: *fd,
//                });
//            }
//        }
//    });
//
//    tracing::span!(tracing::Level::TRACE, "submit-events",).in_scope(|| {
//        server.submit_events();
//    });
}
