//! All the networking related code.

use std::{
    hash::{Hash, Hasher},
    net::ToSocketAddrs,
};

use anyhow::Context;
use derive_more::{Deref, DerefMut, From};
use evenio::prelude::Component;
use libc::iovec;
use sha2::Digest;
use valence_protocol::{uuid::Uuid, CompressionThreshold, Encode, VarInt};

use crate::{
    global::Global,
    net::encoder::PacketWriteInfo,
    singleton::ring::{McBuf, Ring},
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
}

#[derive(Component)]
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
        broadcast: &'a [PacketWriteInfo],
        writers: impl Iterator<Item = RefreshItems<'a>>,
    ) {
        self.server.write_all(global, broadcast, writers);
    }
}

#[repr(packed)]
pub struct RefreshItems<'a> {
    pub write: &'a mut [PacketWriteInfo],
    pub fd: Fd,
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
        broadcast: &'a [PacketWriteInfo],
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
        global: &mut Global,
        broadcast: &'a [PacketWriteInfo],
        writers: impl Iterator<Item = RefreshItems<'a>>,
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

pub const MAX_PACKET_LEN_SIZE: usize = VarInt::MAX_SIZE;

/// The stringified name of the Minecraft version this library currently
/// targets.
pub const MINECRAFT_VERSION: &str = "1.20.1";

/// Get a [`Uuid`] based on the given user's name.
fn offline_uuid(username: &str) -> anyhow::Result<Uuid> {
    let digest = sha2::Sha256::digest(username);

    #[expect(clippy::indexing_slicing, reason = "sha256 is always 32 bytes")]
    let slice = &digest[..16];

    Uuid::from_slice(slice).context("failed to create uuid")
}

mod encoder;

const NUM_PLAYERS: usize = 1024;
const S2C_BUFFER_SIZE: usize = 1024 * 1024 * NUM_PLAYERS;

#[derive(Component, Debug)]
pub struct IoBuf {
    /// The encoding buffer and logic
    enc: encoder::PacketEncoder,
    buf: Ring<MAX_PACKET_SIZE>,
}

impl Default for IoBuf {
    fn default() -> Self {
        Self {
            enc: encoder::PacketEncoder::new(CompressionThreshold(-1)),
            buf: Ring::new(),
        }
    }
}

/// This is useful for the ECS so we can use Single<&mut Broadcast> instead of having to use a marker struct
#[derive(Component, From, Deref, DerefMut)]
pub struct Broadcast(Packets);

#[derive(Component)]
pub struct Packets {
    to_write: Vec<PacketWriteInfo>,
}

impl Packets {
    pub fn append_pre_compression_packet<P>(
        &mut self,
        pkt: &P,
        buf: &mut IoBuf,
    ) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        let compression = buf.enc.compression_threshold();
        // none
        buf.enc.set_compression(CompressionThreshold::DEFAULT);

        let result = buf.enc.append_packet(pkt, &mut buf.buf)?;
        self.to_write.push(result);

        // reset
        buf.enc.set_compression(compression);

        Ok(())
    }

    pub fn append<P>(&mut self, pkt: &P, buf: &mut IoBuf) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        let result = buf.enc.append_packet(pkt, &mut buf.buf)?;
        self.to_write.push(result);
        Ok(())
    }

    pub fn append_raw(&mut self, data: &[u8], buf: &mut IoBuf) -> anyhow::Result<()> {
        buf.buf.append(data);
        Ok(())
    }
}
