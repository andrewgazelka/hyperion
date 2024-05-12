use evenio::{
    event::ReceiverMut,
    fetch::{Fetcher, Single},
};
use tracing::{instrument, log::warn};
use valence_protocol::packets;

use crate::{
    components::{LoginState, LoginStatePendingC2s, LoginStatePendingS2c},
    event::Egress,
    net::{registered_buffer::RegisteredBuffer, encoder::{DataWriteInfo, append_packet_without_compression}, Broadcast, Fd, ServerDef, WriteItem, MINECRAFT_VERSION, PROTOCOL_VERSION},
};

mod status;

#[instrument(skip_all, level = "trace")]
pub fn egress(
    r: ReceiverMut<Egress>,
    mut players: Fetcher<(&Fd, &mut LoginState)>,
    broadcast: Single<&mut Broadcast>,
    mut registered_buffer: Single<&mut RegisteredBuffer>,
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
    let mut event = r.event;
    let server = &mut *event.server;
    let registered_buffer: &mut [u8] = &mut registered_buffer.0;

    let mut buffer_offset = 0;
    tracing::span!(tracing::Level::TRACE, "send",).in_scope(|| {
        for (fd, login) in &mut players {
            tracing::info!("egress: login state: {login:?}");
            let buffer_start = buffer_offset;

            for generator in [status::generate_status_packets] {
                let bytes_written = generator(&mut registered_buffer[buffer_offset..], login).unwrap();
                buffer_offset += bytes_written;
            }

            let len = buffer_offset - buffer_start;
            if len > 0 {
                server.inner.write(WriteItem {
                    info: &DataWriteInfo {
                        // TODO: Check if this breaks any aliasing rules with having a &mut [u8] from
                        // RegisteredBuffer and the kernel reading *const u8 in here
                        // SAFETY: buffer_start should be in bounds of registered_buffer and therefore
                        // not cause overflow
                        start_ptr: unsafe { registered_buffer.as_ptr().add(buffer_start) },
                        len: len as u32,
                    },
                    // It's assumed that RegisteredBuffer is the only buffer registered in this server
                    buffer_idx: 0,
                    fd: *fd,
                });
            }

            // no broadcasting if we are not in play state
//            if *login != LoginState::Play {
//                continue;
//            }

            // todo: append broadcast even if cannot send and have packet prios and stuff
//            if broadcast_len != 0 {
//                pkts.number_sending += 1;
//                server.inner.write(WriteItem {
//                    info: &broadcast.local_to_write,
//                    buffer_idx: broadcast_index,
//                    fd: *fd,
//                });
//            }
        }
    });

    tracing::span!(tracing::Level::TRACE, "submit-events",).in_scope(|| {
        server.submit_events();
    });
}
