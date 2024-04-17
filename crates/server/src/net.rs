//! All the networking related code.

use std::{
    hash::{Hash, Hasher},
    net::ToSocketAddrs,
};

use anyhow::Context;
use arrayvec::CapacityError;
use evenio::handler::Local;
use evenio::prelude::Component;
use libc::iovec;
use sha2::Digest;
use valence_protocol::{uuid::Uuid, CompressionThreshold, Encode};

use crate::{global::Global, singleton::buffer_allocator::BufRef};

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

    fn drain(&mut self, f: impl FnMut(ServerEvent)) {
        self.server.drain(f);
    }

    fn allocate_buffers(&mut self, buffers: &[iovec]) {
        self.server.allocate_buffers(buffers);
    }

    fn write<'a>(&mut self, global: &mut Global, writers: impl Iterator<Item = RefreshItem<'a>>) {
        self.server.write(global, writers);
    }

    fn broadcast(&mut self, buf: &BufRef, fds: impl Iterator<Item = Fd>) {
        self.server.broadcast(buf, fds);
    }

    fn submit_events(&mut self) {
        self.server.submit_events();
    }
}

pub type RefreshItem<'a> = (&'a BufRef, Fd);

pub trait ServerDef {
    fn new(address: impl ToSocketAddrs) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn drain(&mut self, f: impl FnMut(ServerEvent));

    // todo:make unsafe
    fn allocate_buffers(&mut self, buffers: &[iovec]);

    fn write<'a>(&mut self, global: &mut Global, writers: impl Iterator<Item = RefreshItem<'a>>);

    fn broadcast(&mut self, buf: &BufRef, fds: impl Iterator<Item = Fd>);

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

    fn drain(&mut self, _f: impl FnMut(ServerEvent)) {
        unimplemented!("not implemented; use Linux")
    }

    fn allocate_buffers(&mut self, _buffers: &[iovec]) {
        todo!()
    }

    fn write<'a>(&mut self, _global: &mut Global, _writers: impl Iterator<Item = RefreshItem<'a>>) {
        unimplemented!("not implemented; use Linux")
    }

    fn broadcast(&mut self, _buf: &BufRef, _fds: impl Iterator<Item = Fd>) {
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

/// Get a [`Uuid`] based on the given user's name.
fn offline_uuid(username: &str) -> anyhow::Result<Uuid> {
    let digest = sha2::Sha256::digest(username);

    #[expect(clippy::indexing_slicing, reason = "sha256 is always 32 bytes")]
    let slice = &digest[..16];

    Uuid::from_slice(slice).context("failed to create uuid")
}

mod encoder;

#[derive(Component)]
pub struct LocalEncoder {
    /// The encoding buffer and logic
    enc: encoder::PacketEncoder,
}

// TODO: REMOVE
unsafe impl Send for LocalEncoder {}
unsafe impl Sync for LocalEncoder {}


impl LocalEncoder {
    pub fn clear(&mut self) {
        self.enc.buf.clear();
    }

    /// Encode a packet.
    pub fn append<P>(&mut self, pkt: &P, _global: &Global) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        self.enc.append_packet(pkt)?;

        Ok(())
    }

    pub fn set_compression(&mut self, compression_level: CompressionThreshold) {
        self.enc.set_compression(compression_level);
    }

    pub fn new(buffer: BufRef) -> Self {
        Self {
            enc: encoder::PacketEncoder::new(CompressionThreshold(-1), buffer),
        }
    }

    pub const fn buf(&self) -> &BufRef {
        &self.enc.buf
    }

    pub fn append_raw(&mut self, bytes: &[u8], _global: &Global) -> Result<(), CapacityError> {
        self.enc.buf.try_extend_from_slice(bytes)
    }
}
