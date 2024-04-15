//! All the networking related code.

#![expect(clippy::future_not_send, reason = "monoio is not Send")]

use std::{
    io::Write,
    net::ToSocketAddrs,
    ops::{Index, IndexMut, RangeBounds},
    os::fd::AsRawFd,
    slice::SliceIndex,
};

use anyhow::Context;
use bytes::BufMut;
use evenio::prelude::Component;
use monoio::io::{AsyncReadRent, AsyncWriteRent, AsyncWriteRentExt, Splitable};
use sha2::Digest;
use valence_protocol::{uuid::Uuid, Decode, Encode};

use crate::global::Global;

mod buffer;

pub use buffer::*;

#[cfg(target_os = "linux")]
mod linux;

#[derive(Debug)]
struct Fd(
    #[cfg(target_os = "linux")] linux::Fixed,
    #[cfg(target_os = "macos")] (),
);

pub enum ServerEvent<'a> {
    AddPlayer { fd: Fd },
    RemovePlayer { fd: Fd },
    RecvData { fd: Fd, data: &'a [u8] },
}

pub struct Server {
    #[cfg(target_os = "linux")]
    server: linux::Server,
    #[cfg(target_os = "macos")]
    server: NotImplemented,
}

impl ServerDef for Server {
    fn new(address: impl ToSocketAddrs) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        #[cfg(target_os = "linux")]
        {
            Ok(Self {
                server: linux::Server::new(address)?,
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
        self.server.drain(f)
    }

    fn refresh_buffers<'a>(
        &mut self,
        global: &mut Global,
        encoders: impl Iterator<Item = &'a mut Encoder>,
    ) {
        self.server.refresh_buffers(global, encoders)
    }

    fn submit_events(&mut self) {
        self.server.submit_events()
    }
}

pub trait ServerDef {
    fn new(address: impl ToSocketAddrs) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn drain(&mut self, f: impl FnMut(ServerEvent));
    fn refresh_buffers<'a>(
        &mut self,
        global: &mut Global,
        encoders: impl Iterator<Item = &'a mut Encoder>,
    );
    
    fn submit_events(&mut self);
}

struct NotImplemented;

impl ServerDef for NotImplemented {
    fn new(address: impl ToSocketAddrs) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        unimplemented!("not implemented for macOS")
    }

    fn drain(&mut self, _f: impl FnMut(ServerEvent)) {
        unimplemented!("not implemented for macOS")
    }

    fn refresh_buffers<'a>(
        &mut self,
        _global: &mut Global,
        _encoders: impl Iterator<Item = &'a mut Encoder>,
    ) {
        unimplemented!("not implemented for macOS")
    }

    fn submit_events(&mut self) {
        unimplemented!("not implemented for macOS")
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

/// Sent from the I/O thread when it has established a connection with the player through a handshake
pub struct ClientConnection {
    /// The local encoder used by that player
    pub encoder: Encoder,
    /// The name of the player.
    pub name: Box<str>,
    /// The UUID of the player.
    pub uuid: Uuid,
}

mod encoder;

#[derive(Component)]
pub struct Encoder {
    /// The encoding buffer and logic
    enc: encoder::PacketEncoder,

    /// If we should clear the `enc` allocation once we are done sending it off.
    ///
    /// In the future, perhaps we will have a global buffer if it is performant enough.
    deallocate_on_process: bool,
}

impl Encoder {
    /// Encode a packet.
    pub fn append<P>(&mut self, pkt: &P, global: &Global) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        self.enc.append_packet(pkt)?;
        Ok(())
    }

    pub fn append_raw(&mut self, bytes: &[u8], global: &Global) {
        self.enc.buf.extend_from_slice(bytes);
    }

    // /// This sends the bytes to the connection.
    // /// [`PacketEncoder`] can have compression enabled.
    // /// One must make sure the bytes are pre-compressed if compression is enabled.
    // pub fn append(&mut self, bytes: &[u8]) {
    //     trace!("send raw bytes");
    //     self.enc.
    // }
}
