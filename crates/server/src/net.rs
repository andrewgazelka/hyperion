//! All the networking related code.

use std::{cell::RefCell, hash::Hash};

use bytes::BytesMut;
pub use decoder::PacketDecoder;
use derive_more::{Deref, DerefMut};
use evenio::{fetch::Single, handler::HandlerParam, prelude::Component};
use hyperion_proto::ChunkPosition;
use libdeflater::CompressionLvl;
use prost::encoding::{encode_varint, WireType};
use rayon_local::RayonLocal;
use tracing::instrument;

use crate::{
    event::{Scratch, Scratches},
    global::Global,
    net::encoder::PacketEncoder,
};

pub mod proxy;

pub const RING_SIZE: usize = MAX_PACKET_SIZE * 2;

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

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Packets {
    id: u64,
}
impl Packets {
    pub fn id(&self) -> u64 {
        self.id
    }
}

#[derive(HandlerParam, Copy, Clone)]
pub struct Compose<'a> {
    compressor: Single<'a, &'static Compressors>,
    scratch: Single<'a, &'static Scratches>,
    global: Single<'a, &'static Global>,
    io: Single<'a, &'static Io>,
}

impl<'a> Compose<'a> {
    /// Broadcast globally to all players
    ///     
    /// See <https://github.com/andrewgazelka/hyperion-proto/blob/main/src/server_to_proxy.proto#L17-L22>
    pub fn broadcast<'b, P>(&self, packet: &'b P) -> Broadcast<'a, 'b, P>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Broadcast {
            packet,
            optional: false,
            compose: *self,
        }
    }

    pub fn io(&self) -> &Io {
        &self.io
    }

    pub fn broadcast_local<P>(&self, packet: &'a P, center: ChunkPosition) -> BroadcastLocal<'a, P>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        BroadcastLocal {
            packet,
            optional: false,
            compose: *self,
            radius: 0,
            center,
        }
    }

    pub fn unicast<P>(&self, packet: &P, id: Packets) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Unicast {
            packet,
            id: id.id,
            compose: *self,
        }
        .send()
    }

    pub fn multicast<P>(&self, packet: &P, ids: &[Packets]) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Multicast {
            packet,
            compose: *self,
            ids: unsafe { core::slice::from_raw_parts(ids.as_ptr().cast(), ids.len()) },
        }
        .send()
    }

    #[must_use]
    pub fn encoder(&self) -> PacketEncoder {
        let threshold = self.global.shared.compression_threshold;
        PacketEncoder::new(threshold)
    }

    #[must_use]
    pub fn scratch(&self) -> &RefCell<Scratch> {
        self.scratch.get_local()
    }

    #[must_use]
    pub fn compressor(&self) -> &RefCell<libdeflater::Compressor> {
        self.compressor.get_local()
    }
}

/// This is useful for the ECS, so we can use Single<&mut Broadcast> instead of having to use a marker struct
#[derive(Component)]
pub struct Io {
    buffer: RayonLocal<BytesMut>,
    temp_buffer: RayonLocal<Vec<u8>>,
}

struct Broadcast<'a, 'b, P> {
    packet: &'b P,
    optional: bool,
    compose: Compose<'a>,
}

struct Unicast<'a, 'b, P> {
    packet: &'b P,
    id: u64,
    compose: Compose<'a>,
}

impl<'a, 'b, P> Unicast<'a, 'b, P>
where
    P: valence_protocol::Packet + valence_protocol::Encode,
{
    fn send(&self) -> anyhow::Result<()> {
        self.compose
            .io
            .unicast_private(self.packet, self.id, &self.compose)
    }
}

struct Multicast<'a, 'b, P> {
    packet: &'b P,
    ids: &'a [u64],
    compose: Compose<'a>,
}

impl<'a, 'b, P> Multicast<'a, 'b, P>
where
    P: valence_protocol::Packet + valence_protocol::Encode,
{
    fn send(&self) -> anyhow::Result<()> {
        self.compose
            .io
            .multicast_private(self.packet, self.ids, &self.compose)
    }
}

impl<'a, 'b, P> Broadcast<'a, 'b, P> {
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn send(self) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        self.compose
            .io
            .broadcast_private(self.packet, &self.compose, self.optional)
    }
}

struct BroadcastLocal<'a, P> {
    packet: &'a P,
    compose: Compose<'a>,
    radius: u32,
    center: ChunkPosition,
    optional: bool,
}

impl<'a, P> BroadcastLocal<'a, P> {
    fn radius(mut self, radius: u32) -> Self {
        self.radius = radius;
        self
    }

    fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn send(self) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        self.compose.io.broadcast_local_private(
            self.packet,
            &self.compose,
            self.center,
            self.radius,
            self.optional,
        )
    }
}

impl Io {
    #[instrument(skip_all)]
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            buffer: RayonLocal::init_with_defaults(),
            temp_buffer: RayonLocal::init_with_defaults(),
        })
    }

    pub fn split(&mut self) -> impl Iterator<Item = BytesMut> + '_ {
        self.buffer.get_all_mut().into_iter().map(|buf| buf.split())
    }

    unsafe fn encode_packet<P>(&self, packet: &P, compose: &Compose) -> anyhow::Result<&[u8]>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let temp_buffer = self.temp_buffer.get_local_raw();
        let temp_buffer = unsafe { &mut *temp_buffer.get() };

        temp_buffer.clear();

        let compressor = compose.compressor.get_local();
        let mut compressor = compressor.borrow_mut();

        let scratch = compose.scratch.get_local();
        let mut scratch = scratch.borrow_mut();

        compose
            .encoder()
            .append_packet(packet, temp_buffer, &mut *scratch, &mut compressor)?;

        Ok(temp_buffer.as_slice())
    }

    fn broadcast_private<P>(
        &self,
        packet: &P,
        compose: &Compose,
        optional: bool,
    ) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = unsafe { self.encode_packet(packet, compose) }?;
        self.broadcast_raw(bytes, optional);
        Ok(())
    }

    fn broadcast_local_private<P>(
        &self,
        packet: &P,
        compose: &Compose,
        center: ChunkPosition,
        radius: u32,
        optional: bool,
    ) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = unsafe { self.encode_packet(packet, compose) }?;
        self.broadcast_local_raw(bytes, center, radius, optional);
        Ok(())
    }

    fn unicast_private<P>(&self, packet: &P, id: u64, compose: &Compose) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = unsafe { self.encode_packet(packet, compose) }?;
        self.unicast_raw(bytes, id);
        Ok(())
    }

    fn multicast_private<P>(&self, packet: &P, ids: &[u64], compose: &Compose) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = unsafe { self.encode_packet(packet, compose) }?;
        self.multicast_raw(bytes, ids);
        Ok(())
    }

    pub fn broadcast_local_raw(
        &self,
        data: &[u8],
        center: ChunkPosition,
        radius: u32,
        optional: bool,
    ) {
        const TAG: u64 = 3;

        let buffer = self.buffer.get_local_raw();
        let buffer = unsafe { &mut *buffer.get() };

        encode_varint(TAG, buffer);

        prost::encoding::encode_key(1, WireType::LengthDelimited, buffer);
        encode_varint(data.len() as u64, buffer);
        buffer.extend_from_slice(data);

        if radius != 0 {
            prost::encoding::uint32::encode(2, &radius, buffer);
        }

        prost::encoding::message::encode(3, &center, buffer);

        if optional {
            prost::encoding::bool::encode(4, &optional, buffer);
        }
    }

    pub fn broadcast_raw(&self, data: &[u8], optional: bool) {
        const TAG: u64 = 2;

        let buffer = self.buffer.get_local_raw();
        let buffer = unsafe { &mut *buffer.get() };

        encode_varint(TAG, buffer);

        prost::encoding::encode_key(1, WireType::LengthDelimited, buffer);
        encode_varint(data.len() as u64, buffer);
        buffer.extend_from_slice(data);

        if optional {
            prost::encoding::bool::encode(2, &optional, buffer);
        }
    }

    pub fn unicast_raw(&self, data: &[u8], id: u64) {
        const TAG: u64 = 5;

        let buffer = self.buffer.get_local_raw();
        let buffer = unsafe { &mut *buffer.get() };

        encode_varint(TAG, buffer);

        prost::encoding::encode_key(1, WireType::LengthDelimited, buffer);
        encode_varint(data.len() as u64, buffer);
        buffer.extend_from_slice(data);

        prost::encoding::uint64::encode(2, &id, buffer);
    }

    pub fn multicast_raw(&self, data: &[u8], ids: &[u64]) {
        const TAG: u64 = 4;

        let buffer = self.buffer.get_local_raw();
        let buffer = unsafe { &mut *buffer.get() };

        encode_varint(TAG, buffer);

        prost::encoding::encode_key(1, WireType::LengthDelimited, buffer);
        encode_varint(ids.len() as u64, buffer);
        buffer.extend_from_slice(data);

        prost::encoding::uint64::encode_packed(2, ids, buffer);
    }
}
