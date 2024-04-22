//! All the networking related code.

use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
    net::ToSocketAddrs,
};

use derive_more::{Deref, DerefMut, From};
use evenio::prelude::Component;
use libc::iovec;
use valence_protocol::{CompressionThreshold, Encode};

use crate::{
    events::ScratchBuffer,
    global::Global,
    net::encoder::PacketWriteInfo,
    singleton::ring::{Buf, Ring},
};

#[cfg(target_os = "linux")]
mod linux;

#[derive(Debug, Copy, Clone, Component)]
pub struct Fd(
    #[cfg(target_os = "linux")] linux::Fixed,
    #[cfg(target_os = "macos")] (),
);

#[cfg(target_os = "linux")]
impl Hash for Fd {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0 .0.hash(state);
    }
}

#[cfg(not(target_os = "linux"))]
impl PartialEq for Fd {
    fn eq(&self, _other: &Self) -> bool {
        unimplemented!()
    }
}

#[cfg(not(target_os = "linux"))]
impl Eq for Fd {}

#[cfg(not(target_os = "linux"))]
impl Hash for Fd {
    fn hash<H: Hasher>(&self, _state: &mut H) {
        unimplemented!()
    }
}

#[cfg(target_os = "linux")]
impl PartialEq for Fd {
    fn eq(&self, other: &Self) -> bool {
        self.0 .0 == other.0 .0
    }
}

#[cfg(target_os = "linux")]
impl Eq for Fd {}

#[allow(unused, reason = "these are used on linux")]
pub enum ServerEvent<'a> {
    AddPlayer { fd: Fd },
    RemovePlayer { fd: Fd },
    RecvData { fd: Fd, data: &'a [u8] },
    SentData { fd: Fd },
}

pub struct Server {
    #[cfg(target_os = "linux")]
    server: linux::LinuxServer,
    #[cfg(not(target_os = "linux"))]
    server: NotImplemented,
}

impl ServerDef for Server {
    #[allow(unused, reason = "this has to do with cross-platform code")]
    fn new(address: impl ToSocketAddrs) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        #[cfg(target_os = "linux")]
        {
            Ok(Self {
                server: linux::LinuxServer::new(address)?,
            })
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                server: NotImplemented,
            })
        }
    }

    fn drain(&mut self, f: impl FnMut(ServerEvent)) -> std::io::Result<()> {
        self.server.drain(f)
    }

    fn allocate_buffers(&mut self, buffers: &[iovec]) {
        self.server.allocate_buffers(buffers);
    }

    fn submit_events(&mut self) {
        self.server.submit_events();
    }

    /// Impl with local sends BEFORE broadcasting
    fn write_all<'a>(
        &mut self,
        global: &mut Global,
        writers: impl Iterator<Item = RefreshItems<'a>>,
    ) {
        self.server.write_all(global, writers);
    }
}

#[allow(unused, reason = "this is used on linux")]
pub struct RefreshItems<'a> {
    pub write: &'a VecDeque<PacketWriteInfo>,
    pub fd: Fd,
    pub broadcast: bool,
}

pub trait ServerDef {
    fn new(address: impl ToSocketAddrs) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn drain(&mut self, f: impl FnMut(ServerEvent)) -> std::io::Result<()>;

    // todo:make unsafe
    fn allocate_buffers(&mut self, buffers: &[iovec]);

    fn write_all<'a>(
        &mut self,
        global: &mut Global,
        writers: impl Iterator<Item = RefreshItems<'a>>,
    );

    fn submit_events(&mut self);
}

struct NotImplemented;

impl ServerDef for NotImplemented {
    fn new(_address: impl ToSocketAddrs) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        unimplemented!("not implemented; use Linux")
    }

    fn drain(&mut self, _f: impl FnMut(ServerEvent)) -> std::io::Result<()> {
        unimplemented!("not implemented; use Linux")
    }

    fn allocate_buffers(&mut self, _buffers: &[iovec]) {
        unimplemented!("not implemented; use Linux")
    }

    fn write_all<'a>(
        &mut self,
        _global: &mut Global,
        _writers: impl Iterator<Item = RefreshItems<'a>>,
    ) {
        unimplemented!("not implemented; use Linux")
    }

    fn submit_events(&mut self) {
        unimplemented!("not implemented; use Linux")
    }
}

/// The Minecraft protocol version this library currently targets.
pub const PROTOCOL_VERSION: i32 = 763;

/// The maximum number of bytes that can be sent in a single packet.
pub const MAX_PACKET_SIZE: usize = 0x001F_FFFF;

/// The stringified name of the Minecraft version this library currently
/// targets.
pub const MINECRAFT_VERSION: &str = "1.20.1";

mod encoder;

const NUM_PLAYERS: usize = 1024;
const S2C_BUFFER_SIZE: usize = 1024 * 1024 * NUM_PLAYERS;

#[derive(Component, Debug)]
pub struct IoBuf {
    /// The encoding buffer and logic
    enc: encoder::PacketEncoder,
    buf: Ring,
}

impl IoBuf {
    pub fn new(threshold: CompressionThreshold) -> Self {
        Self {
            enc: encoder::PacketEncoder::new(threshold, flate2::Compression::new(4)),
            buf: Ring::new(S2C_BUFFER_SIZE),
        }
    }

    pub fn buf_mut(&mut self) -> &mut Ring {
        &mut self.buf
    }
}

/// This is useful for the ECS so we can use Single<&mut Broadcast> instead of having to use a marker struct
#[derive(Component, From, Deref, DerefMut, Default)]
pub struct Broadcast(Packets);

/// Stores indices of packets
#[derive(Component, Default)]
pub struct Packets {
    to_write: VecDeque<PacketWriteInfo>,
    can_send: bool,
}

impl Packets {
    pub fn get_write(&mut self) -> &mut VecDeque<PacketWriteInfo> {
        &mut self.to_write
    }

    pub const fn can_send(&self) -> bool {
        self.can_send
    }

    pub fn set_successfully_sent(&mut self) {
        self.can_send = true;
    }

    pub fn set_sending(&mut self) {
        self.can_send = false;
    }

    pub fn clear(&mut self) {
        self.to_write.clear();
    }

    fn push(&mut self, writer: PacketWriteInfo) {
        if let Some(last) = self.to_write.back_mut() {
            let start_pointer_if_contiguous = unsafe { last.start_ptr.add(last.len as usize) };
            if start_pointer_if_contiguous == writer.start_ptr {
                last.len += writer.len;
                return;
            }
        }

        self.to_write.push_back(writer);
    }

    pub fn append_pre_compression_packet<P>(
        &mut self,
        pkt: &P,
        buf: &mut IoBuf,
        scratch: &mut impl ScratchBuffer,
    ) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        let compression = buf.enc.compression_threshold();
        // none
        buf.enc.set_compression(CompressionThreshold::DEFAULT);

        let result = buf.enc.append_packet(pkt, &mut buf.buf, scratch)?;

        self.push(result);

        // reset
        buf.enc.set_compression(compression);

        Ok(())
    }

    pub fn append<P>(
        &mut self,
        pkt: &P,
        buf: &mut IoBuf,
        scratch: &mut impl ScratchBuffer,
    ) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        let result = buf.enc.append_packet(pkt, &mut buf.buf, scratch)?;

        self.push(result);
        Ok(())
    }

    pub fn append_raw(&mut self, data: &[u8], buf: &mut IoBuf) {
        let start_ptr = buf.buf.append(data);

        let writer = PacketWriteInfo {
            start_ptr,
            len: data.len() as u32,
        };

        self.push(writer);
    }
}

#[cfg(test)]
mod tests {
    use bumpalo::Bump;
    use valence_protocol::{packets::login::LoginHelloC2s, Bounded};

    use super::*;
    use crate::events::Scratch;

    #[test]
    fn test_append_pre_compression_packet() {
        let mut buf = IoBuf::new(CompressionThreshold::DEFAULT);
        let mut packets = Packets::default();

        let pkt = LoginHelloC2s {
            username: Bounded::default(),
            profile_id: None,
        };

        let bump = Bump::new();
        let mut scratch = Scratch::from(&bump);

        packets
            .append_pre_compression_packet(&pkt, &mut buf, &mut scratch)
            .unwrap();

        assert_eq!(packets.to_write().len(), 1);

        let len = packets.to_write()[0].len;

        assert_eq!(len, 4); // Packet length for an empty LoginHelloC2s
    }
    #[test]
    fn test_append_packet() {
        let mut buf = IoBuf::new(CompressionThreshold::DEFAULT);
        let mut packets = Packets::default();

        let pkt = LoginHelloC2s {
            username: Bounded::default(),
            profile_id: None,
        };

        let bump = Bump::new();
        let mut scratch = Scratch::from(&bump);
        packets.append(&pkt, &mut buf, &mut scratch).unwrap();

        assert_eq!(packets.to_write().len(), 1);
        let len = packets.to_write()[0].len;
        assert_eq!(len, 4); // Packet length for an empty LoginHelloC2s
    }

    #[test]
    fn test_append_raw() {
        let mut buf = IoBuf::new(CompressionThreshold::DEFAULT);
        let mut packets = Packets::default();

        let data = b"Hello, world!";
        packets.append_raw(data, &mut buf);

        assert_eq!(packets.to_write().len(), 1);

        let len = packets.to_write()[0].len;
        assert_eq!(len, data.len() as u32);
    }

    #[test]
    fn test_clear() {
        let mut buf = IoBuf::new(CompressionThreshold::DEFAULT);
        let mut packets = Packets::default();

        let pkt = LoginHelloC2s {
            username: Bounded::default(),
            profile_id: None,
        };

        let bump = Bump::new();
        let mut scratch = Scratch::from(&bump);

        packets.append(&pkt, &mut buf, &mut scratch).unwrap();
        assert_eq!(packets.to_write().len(), 1);

        packets.clear();
        assert_eq!(packets.to_write().len(), 0);
    }

    #[test]
    fn test_contiguous_packets() {
        let mut buf = IoBuf::new(CompressionThreshold::DEFAULT);
        let mut packets = Packets::default();

        let pkt1 = LoginHelloC2s {
            username: Bounded::default(),
            profile_id: None,
        };
        let pkt2 = LoginHelloC2s {
            username: Bounded::default(),
            profile_id: None,
        };

        let bump = Bump::new();
        let mut scratch = Scratch::from(&bump);

        packets
            .append_pre_compression_packet(&pkt1, &mut buf, &mut scratch)
            .unwrap();
        packets
            .append_pre_compression_packet(&pkt2, &mut buf, &mut scratch)
            .unwrap();

        assert_eq!(packets.to_write().len(), 1);

        let len = packets.to_write()[0].len;
        assert_eq!(len, 8); // Combined length of both packets
    }
}
