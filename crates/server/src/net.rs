//! All the networking related code.

use std::{cell::RefCell, collections::VecDeque, hash::Hash, net::ToSocketAddrs, net::SocketAddr, sync::Mutex};

use anyhow::Context;
use arrayvec::ArrayVec;
use derive_more::{Deref, DerefMut};
use evenio::{fetch::Single, handler::HandlerParam, prelude::Component};
use libc::iovec;
use libdeflater::CompressionLvl;
use tracing::{debug, instrument};

use crate::{global::Global, net::encoder::DataWriteInfo};

pub mod buffers;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(target_os = "linux"))]
mod generic;

#[derive(Debug, Copy, Clone, Component, PartialEq, Eq, Hash)]
pub struct Fd(
    #[cfg(target_os = "linux")] linux::Fixed,
    #[cfg(not(target_os = "linux"))] usize,
);

pub const RING_SIZE: usize = MAX_PACKET_SIZE * 2;

#[allow(unused, reason = "these are used on linux")]
pub enum ServerEvent<'a> {
    AddPlayer { fd: Fd },
    RemovePlayer { fd: Fd },
    RecvData { fd: Fd, data: &'a [u8] },
    SentData { fd: Fd, bytes_sent: u32 },
}

pub struct Server {
    #[cfg(target_os = "linux")]
    pub inner: linux::LinuxServer,
    #[cfg(not(target_os = "linux"))]
    pub inner: generic::GenericServer,
}

impl ServerDef for Server {
    #[allow(unused, reason = "this has to do with cross-platform code")]
    fn new(address: SocketAddr) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let inner = {
            #[cfg(target_os = "linux")]
            {
                linux::LinuxServer::new(address)?
            }

            #[cfg(not(target_os = "linux"))]
            {
                generic::GenericServer::new(address)?
            }
        };

        Ok(Self { inner })
    }

    fn drain(&mut self) -> impl Iterator<Item = ServerEvent> {
        self.inner.drain()
    }

    unsafe fn register_buffers(&mut self, buffers: &[iovec]) {
        for (idx, elem) in buffers.iter().enumerate() {
            let ptr = elem.iov_base as *const u8;
            let len = elem.iov_len;
            let len_readable = humansize::SizeFormatter::new(len, humansize::BINARY);
            debug!("buffer {idx} {ptr:?} of len {len} = {len_readable}");
        }

        self.inner.register_buffers(buffers);
    }

    fn submit_events(&mut self) {
        self.inner.submit_events();
    }

    fn write(&mut self, item: WriteItem) {
        self.inner.write(item);
    }
}

#[derive(Debug, Copy, Clone)]
pub struct GlobalPacketWriteInfo {
    pub start_ptr: *const u8,
    pub len: u32,
    pub buffer_idx: u16,
}

unsafe impl Send for GlobalPacketWriteInfo {}
unsafe impl Sync for GlobalPacketWriteInfo {}

#[allow(unused, reason = "this is used on linux")]
pub struct WriteItem<'a> {
    pub info: &'a DataWriteInfo,
    pub buffer_idx: u16,
    pub fd: Fd,
}

pub trait ServerDef {
    fn new(address: SocketAddr) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn drain(&mut self) -> impl Iterator<Item = ServerEvent>;

    /// # Safety
    /// todo: not completely sure about all the invariants here
    unsafe fn register_buffers(&mut self, buffers: &[iovec]);

    fn write(&mut self, item: WriteItem);

    fn submit_events(&mut self);
}

/// The Minecraft protocol version this library currently targets.
pub const PROTOCOL_VERSION: i32 = 763;

// todo: this is one off.. why?
// pub const MAX_PACKET_SIZE: usize = 0x001F_FFFF;
/// The maximum number of bytes that can be sent in a single packet.
pub const MAX_PACKET_SIZE: usize = valence_protocol::MAX_PACKET_SIZE as usize;

/// The stringified name of the Minecraft version this library currently
/// targets.
pub const MINECRAFT_VERSION: &str = "1.20.1";

mod decoder;
pub mod encoder;

pub use decoder::PacketDecoder;
use rayon_local::RayonLocal;

use crate::{
    event::Scratches,
    net::{
        buffers::{BufRef, BufferAllocator},
        encoder::{append_packet_without_compression, PacketEncoder},
    },
};

// 128 MiB * num_cores
pub const S2C_BUFFER_SIZE: usize = 1024 * 1024 * 128;

#[derive(Component, Deref, DerefMut)]
pub struct Compressors {
    compressors: RayonLocal<RefCell<libdeflater::Compressor>>,
}

impl Compressors {
    #[must_use]
    pub fn new(level: CompressionLvl) -> Self {
        Self {
            compressors: RayonLocal::init(|| libdeflater::Compressor::new(level).into()),
        }
    }
}

#[derive(HandlerParam, Copy, Clone)]
pub struct Compose<'a> {
    pub compressor: Single<'a, &'static Compressors>,
    pub scratch: Single<'a, &'static Scratches>,
    pub global: Single<'a, &'static Global>,
}

impl<'a> Compose<'a> {
    #[must_use]
    pub fn encoder(&self) -> PacketEncoder {
        let threshold = self.global.shared.compression_threshold;
        PacketEncoder::new(threshold)
    }
}

/// Stores indices of packets
#[derive(Component)]
pub struct Packets {
    to_write: Mutex<Vec<u8>>,
    currently_sending: bool
}

impl Packets {
    /// To avoid locking a mutex, `other` is borrowed as `mut`, but `other` is not changed.
    pub fn extend(&mut self, other: &mut Self) {
        let this = self.to_write.get_mut().unwrap();
        let other = other.to_write.get_mut().unwrap();

        this.extend_from_slice(other);
    }

    pub fn set_successfully_sent(&mut self, bytes_sent: u32) {
        debug_assert!(
            self.currently_sending,
            "set_successfully_sent was called while not waiting for a send completion"
        );
        self.currently_sending = false;

        let buf = self.to_write.get_mut().unwrap();
        // Remove all data that was sent. Any data that was unsent this time will be sent
        // on the next sent.
        buf.drain(0..(bytes_sent as usize));
    }

    /// `f` is called with the data to send. If there is no data that needs to be sent or data
    /// cannot be sent at this moment, `f` is not called and nothing else is done. `f` should not
    /// panic.
    pub fn prepare_for_send<F: FnOnce(&[u8])>(&mut self, mut f: F) {
        if self.currently_sending {
            // Since this needs to handle truncated writes which are not treated as an error by the
            // kernel, this cannot send multiple writes at once.
            return;
        }

        let to_write = self.to_write.get_mut().unwrap();
        if to_write.is_empty() {
            // Nothing needs to be sent
            return;
        }

        self.currently_sending = true;
        f(to_write);
    }

    fn push(&self, writer: PacketWriteInfo) {
        let to_write = self.to_write.get_mut().unwrap();

        // TODO: This is unsound because this function doesn't have safety contract
        let data = unsafe { std::slice::from_raw_parts(writer.start_ptr, writer.len as usize) };
        to_write.extend_from_slice(data);
    }

    pub fn add_successful_send(&mut self, d_count: usize) {
        self.number_sending += d_count as u8;
    }

    pub fn get_write_mut(&mut self) -> &mut ArrayVec<DataWriteInfo, 2> {
        &mut self.local_to_write
    }

    #[must_use]
    pub const fn can_send(&self) -> bool {
        self.number_sending == 0
    }
}

#[derive(Debug, Default)]
pub struct LocalBroadcast {
    pub data: Vec<u8>,
}

/// This is useful for the ECS so we can use Single<&mut Broadcast> instead of having to use a marker struct
#[derive(Component)]
pub struct Broadcast {
    pub buffer: BufRef,
    pub local_to_write: DataWriteInfo,
    packets: RayonLocal<LocalBroadcast>,
}

impl Broadcast {
    #[instrument(skip_all, name = "Broadcast::new")]
    pub fn new(allocator: &mut BufferAllocator) -> anyhow::Result<Self> {
        let buffer = allocator.obtain().unwrap();
        // trace!("initializing broadcast buffers");
        // todo: try_init
        let packets = RayonLocal::init_with_defaults();

        Ok(Self {
            packets,
            buffer,
            local_to_write: DataWriteInfo::NULL,
        })
    }

    pub fn packets_mut(&mut self) -> &mut RayonLocal<LocalBroadcast> {
        &mut self.packets
    }

    #[must_use]
    pub const fn packets(&self) -> &RayonLocal<LocalBroadcast> {
        &self.packets
    }

    pub fn append<P>(&self, pkt: &P, compose: &Compose) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        // trace!("broadcasting {}", P::NAME);

        let scratch = compose.scratch.get_local();
        let mut scratch = scratch.borrow_mut();

        let compressor = compose.compressor.get_local();
        let mut compressor = compressor.borrow_mut();

        let encoder = compose.encoder();

        let local = self.packets.get_local_raw();
        let local = unsafe { &mut *local.get() };

        let buf = &mut local.data;

        encoder.append_packet(pkt, buf, &mut *scratch, &mut compressor)?;

        self.push(result);
        Ok(())
    }

    pub fn append_raw(&self, data: &[u8]) {
        let local = self.packets.get_local_raw();
        let local = unsafe { &mut *local.get() };

        let writer = PacketWriteInfo {
            start_ptr,
            len: data.len() as u32,
        };

        self.push(writer);
    }
}

// #[cfg(test)]
// mod tests {
//     use bumpalo::Bump;
//     use valence_protocol::{packets::login::LoginHelloC2s, Bounded};
//
//     use super::*;
//     use crate::events::Scratch;
//
//     #[test]
//     fn test_append_pre_compression_packet() {
//         let mut buf = IoBuf::new(
//             CompressionThreshold::DEFAULT,
//             CompressionLvl::new(4).unwrap(),
//         );
//         let mut packets = Packets::default();
//
//         let pkt = LoginHelloC2s {
//             username: Bounded::default(),
//             profile_id: None,
//         };
//
//         let bump = Bump::new();
//         let mut scratch = Scratch::from(&bump);
//
//         packets
//             .append_pre_compression_packet(&pkt, &mut buf)
//             .unwrap();
//
//         assert_eq!(packets.get_write().len(), 1);
//
//         let len = packets.get_write()[0].len;
//
//         assert_eq!(len, 4); // Packet length for an empty LoginHelloC2s
//     }
//     #[test]
//     fn test_append_packet() {
//         let mut buf = IoBuf::new(
//             CompressionThreshold::DEFAULT,
//             CompressionLvl::new(4).unwrap(),
//         );
//         let mut packets = Packets::default();
//
//         let pkt = LoginHelloC2s {
//             username: Bounded::default(),
//             profile_id: None,
//         };
//
//         let bump = Bump::new();
//         let mut scratch = Scratch::from(&bump);
//         packets.append(&pkt, &mut buf, &mut scratch).unwrap();
//
//         assert_eq!(packets.get_write().len(), 1);
//         let len = packets.get_write()[0].len;
//         assert_eq!(len, 4); // Packet length for an empty LoginHelloC2s
//     }
//
//     #[test]
//     fn test_append_raw() {
//         let mut buf = IoBuf::new(
//             CompressionThreshold::DEFAULT,
//             CompressionLvl::new(4).unwrap(),
//         );
//         let mut packets = Packets::default();
//
//         let data = b"Hello, world!";
//         packets.append_raw(data, &mut buf);
//
//         assert_eq!(packets.get_write().len(), 1);
//
//         let len = packets.get_write()[0].len;
//         assert_eq!(len, data.len() as u32);
//     }
//
//     #[test]
//     fn test_clear() {
//         let mut buf = IoBuf::new(
//             CompressionThreshold::DEFAULT,
//             CompressionLvl::new(4).unwrap(),
//         );
//         let mut packets = Packets::default();
//
//         let pkt = LoginHelloC2s {
//             username: Bounded::default(),
//             profile_id: None,
//         };
//
//         let bump = Bump::new();
//         let mut scratch = Scratch::from(&bump);
//
//         packets.append(&pkt, &mut buf, &mut scratch).unwrap();
//         assert_eq!(packets.get_write().len(), 1);
//
//         packets.clear();
//         assert_eq!(packets.get_write().len(), 0);
//     }
//
//     #[test]
//     fn test_contiguous_packets() {
//         let mut buf = IoBuf::new(
//             CompressionThreshold::DEFAULT,
//             CompressionLvl::new(4).unwrap(),
//         );
//         let mut packets = Packets::default();
//
//         let pkt1 = LoginHelloC2s {
//             username: Bounded::default(),
//             profile_id: None,
//         };
//         let pkt2 = LoginHelloC2s {
//             username: Bounded::default(),
//             profile_id: None,
//         };
//
//         let bump = Bump::new();
//         let mut scratch = Scratch::from(&bump);
//
//         packets
//             .append_pre_compression_packet(&pkt1, &mut buf, &mut scratch)
//             .unwrap();
//         packets
//             .append_pre_compression_packet(&pkt2, &mut buf, &mut scratch)
//             .unwrap();
//
//         assert_eq!(packets.get_write().len(), 1);
//
//         let len = packets.get_write()[0].len;
//         assert_eq!(len, 8); // Combined length of both packets
//     }
// }
