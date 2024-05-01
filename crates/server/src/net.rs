//! All the networking related code.

use std::{
    cell::RefCell,
    collections::HashSet,
    hash::Hash,
    net::SocketAddr,
    sync::{atomic, atomic::AtomicUsize},
};

use anyhow::Context;
use derive_more::{Deref, DerefMut};
use evenio::{
    fetch::Single,
    handler::HandlerParam,
    prelude::{Component, EntityId},
};
use libc::iovec;
use libdeflater::CompressionLvl;
use tracing::{debug, instrument, span, trace, Level};

use crate::{global::Global, net::encoder::PacketWriteInfo};

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

#[allow(unused, reason = "these are used on linux")]
pub enum ServerEvent {
    AddPlayer { fd: Fd },
    RemovePlayer { fd: Fd },
    RecvData { fd: Fd, data: &'static [u8] },
    SentData { fd: Fd },
}

#[derive(Deref, DerefMut)]
pub struct Servers {
    inner: RayonLocal<Server>,
}

impl Servers {
    #[must_use]
    pub fn new(address: SocketAddr) -> Self {
        let inner = RayonLocal::init(|| Server::new(address).unwrap());
        Self { inner }
    }

    pub fn allocate_buffers(&mut self, buffers: &[iovec]) {
        for (idx, elem) in self.inner.iter_mut().enumerate() {
            span!(Level::INFO, "server", idx = idx).in_scope(|| {
                elem.allocate_buffers(buffers);
            });
        }
    }
}

pub struct Server {
    #[cfg(target_os = "linux")]
    pub inner: linux::LinuxServer,
    #[cfg(not(target_os = "linux"))]
    pub inner: generic::GenericServer,

    pub fd_ids: HashSet<EntityId>,
}

// todo: remove
unsafe impl Send for Server {}
unsafe impl Sync for Server {}

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

        Ok(Self {
            inner,
            fd_ids: HashSet::new(),
        })
    }

    fn drain(&mut self, f: impl FnMut(ServerEvent)) -> std::io::Result<()> {
        self.inner.drain(f)
    }

    fn allocate_buffers(&mut self, buffers: &[iovec]) {
        for (idx, elem) in buffers.iter().enumerate() {
            let ptr = elem.iov_base as *const u8;
            let len = elem.iov_len;
            let len_readable = humansize::SizeFormatter::new(len, humansize::BINARY);
            debug!("buffer {idx} {ptr:?} of len {len} = {len_readable}");
        }

        self.inner.allocate_buffers(buffers);
    }

    fn submit_events(&mut self) {
        self.inner.submit_events();
    }

    fn write(&mut self, item: WriteItem) {
        self.inner.write(item);
    }
}

pub struct GlobalPacketWriteInfo {
    pub start_ptr: *const u8,
    pub len: u32,
    pub buffer_idx: u16,
}

unsafe impl Send for GlobalPacketWriteInfo {}
unsafe impl Sync for GlobalPacketWriteInfo {}

#[allow(unused, reason = "this is used on linux")]
pub struct WriteItem<'a> {
    pub local: &'a mut Vec<PacketWriteInfo>,
    pub global: Option<&'a Vec<GlobalPacketWriteInfo>>,
    pub buffer_idx: u16,
    pub fd: Fd,
}

pub trait ServerDef {
    fn new(address: SocketAddr) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn drain(&mut self, f: impl FnMut(ServerEvent)) -> std::io::Result<()>;

    // todo:make unsafe
    fn allocate_buffers(&mut self, buffers: &[iovec]);

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
use crate::net::buffers::BufferAllocators;

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
    buffer: BufRef,
    local_to_write: Vec<PacketWriteInfo>,
    number_sending: AtomicUsize,
}

impl Packets {
    #[instrument(skip_all)]
    pub fn new(allocator: &mut BufferAllocator) -> anyhow::Result<Self> {
        Ok(Self {
            buffer: allocator.obtain().context("failed to obtain buffer")?,
            local_to_write: Vec::new(),
            number_sending: AtomicUsize::new(0),
        })
    }

    pub fn index(&self) -> u16 {
        self.buffer.index()
    }

    pub fn append<P>(&mut self, pkt: &P, compose: &Compose) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let scratch = compose.scratch.get_local();
        let mut scratch = scratch.borrow_mut();

        let compressor = compose.compressor.get_local();
        let mut compressor = compressor.borrow_mut();

        let enc = compose.encoder();

        let result = enc.append_packet(pkt, &mut *self.buffer, &mut *scratch, &mut compressor)?;

        self.push(result);
        Ok(())
    }

    pub fn append_pre_compression_packet<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        // todo: why need DerefMut?
        let buf = &mut self.buffer;
        let buf = &mut **buf;

        let result = append_packet_without_compression(pkt, buf)?;

        trace!("without compression: {result:?}");

        self.push(result);
        Ok(())
    }

    pub fn append_raw(&mut self, data: &[u8]) {
        let buffer = &mut *self.buffer;
        let start_ptr = buffer.append(data);

        let writer = PacketWriteInfo {
            start_ptr,
            len: data.len() as u32,
        };

        self.push(writer);
    }

    fn push(&mut self, writer: PacketWriteInfo) {
        let to_write = &mut self.local_to_write;

        if let Some(last) = to_write.last_mut() {
            let start_pointer_if_contiguous = unsafe { last.start_ptr.add(last.len as usize) };
            if start_pointer_if_contiguous == writer.start_ptr {
                last.len += writer.len;
                return;
            }
        }

        to_write.push(writer);
    }

    pub fn set_successfully_sent(&self, d_count: usize) {
        debug_assert!(
            self.number_sending.load(atomic::Ordering::Relaxed) > 0,
            "somehow number sending is 0 even though we just marked a successful send"
        );

        self.number_sending
            .fetch_sub(d_count, atomic::Ordering::Relaxed);
    }

    pub fn get_write_mut(&mut self) -> &mut Vec<PacketWriteInfo> {
        &mut self.local_to_write
    }

    #[must_use]
    pub fn can_send(&self, broadcast_count: usize) -> bool {
        if self.number_sending.load(atomic::Ordering::Relaxed) != 0 {
            return false;
        }

        let total_len = self.local_to_write.len() + broadcast_count;
        total_len > 0
    }

    pub fn prepare_for_send(&mut self, extra: usize) -> usize {
        debug_assert!(
            self.number_sending.load(atomic::Ordering::Relaxed) == 0,
            "number sending is not 0 even though we are preparing for send"
        );
        let count = self.local_to_write.len() + extra;
        self.number_sending = AtomicUsize::new(count);
        count
    }
}

pub struct LocalBroadcast {
    pub buffer: BufRef,
    pub local_to_write: Vec<PacketWriteInfo>,
}

/// This is useful for the ECS so we can use Single<&mut Broadcast> instead of having to use a marker struct
#[derive(Component, Deref, DerefMut)]
pub struct Broadcast {
    packets: RayonLocal<LocalBroadcast>,
}

impl Broadcast {
    #[instrument(skip_all)]
    pub fn new(allocator: &mut BufferAllocators) -> anyhow::Result<Self> {
        trace!("initializing broadcast buffers");
        // todo: try_init
        let packets = RayonLocal::init_with_index(|idx| {
            let allocator = allocator.get_mut(idx).unwrap();
            let buffer = allocator.obtain().unwrap();
            LocalBroadcast {
                buffer,
                local_to_write: Vec::new(),
            }
        });

        Ok(Self { packets })
    }

    pub fn clear(&mut self) {
        for packet in &mut self.packets {
            packet.local_to_write.clear();
        }
    }

    fn push(&self, writer: PacketWriteInfo) {
        let to_write = self.packets.get_local_raw();
        let to_write = unsafe { &mut *to_write.get() };

        if let Some(last) = to_write.local_to_write.last_mut() {
            let start_pointer_if_contiguous = unsafe { last.start_ptr.add(last.len as usize) };
            if start_pointer_if_contiguous == writer.start_ptr {
                last.len += writer.len;
                return;
            }
        }

        to_write.local_to_write.push(writer);
    }

    pub fn append_pre_compression_packet<P>(&self, pkt: &P) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let local = self.packets.get_local_raw();
        let local = unsafe { &mut *local.get() };

        let buf = &mut *local.buffer;

        let result = append_packet_without_compression(pkt, buf)?;

        trace!("without compression: {result:?}");

        self.push(result);

        Ok(())
    }

    pub fn append<P>(&self, pkt: &P, compose: &Compose) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let scratch = compose.scratch.get_local();
        let mut scratch = scratch.borrow_mut();

        let compressor = compose.compressor.get_local();
        let mut compressor = compressor.borrow_mut();

        let encoder = compose.encoder();

        let local = self.packets.get_local_raw();
        let local = unsafe { &mut *local.get() };

        let buf = &mut *local.buffer;

        let result = encoder.append_packet(pkt, buf, &mut *scratch, &mut compressor)?;

        self.push(result);
        Ok(())
    }

    pub fn append_raw(&self, data: &[u8]) {
        let local = self.packets.get_local_raw();
        let local = unsafe { &mut *local.get() };

        let buf = &mut *local.buffer;

        let start_ptr = buf.append(data);

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
