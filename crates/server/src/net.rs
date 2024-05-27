//! All the networking related code.

use std::{borrow::Cow, cell::RefCell, hash::Hash};

use bytes::BytesMut;
pub use decoder::PacketDecoder;
use derive_more::{Constructor, Deref, DerefMut};
use evenio::{fetch::Single, handler::HandlerParam, prelude::Component};
use hyperion_proto::ChunkPosition;
use libdeflater::CompressionLvl;
use prost::Message;
use rayon_local::RayonLocal;

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

impl Default for Compressors {
    fn default() -> Self {
        Self::new(CompressionLvl::default())
    }
}

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Hash, Constructor)]
pub struct Packets {
    stream_id: u64,
}
impl Packets {
    #[must_use]
    pub const fn stream(&self) -> u64 {
        self.stream_id
    }
}

#[derive(HandlerParam, Copy, Clone)]
pub struct Compose<'a> {
    pub compressor: Single<&'a Compressors>,
    pub scratch: Single<&'a Scratches>,
    pub global: Single<&'a Global>,
    pub io_buf: Single<&'a IoBuf>,
}

impl<'a> Compose<'a> {
    /// Broadcast globally to all players
    ///
    /// See <https://github.com/andrewgazelka/hyperion-proto/blob/main/src/server_to_proxy.proto#L17-L22>
    pub const fn broadcast<'b, P>(&self, packet: &'b P) -> Broadcast<'a, 'b, 'static, P>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Broadcast {
            packet,
            optional: false,
            compose: *self,
            exclude: Cow::Borrowed(&[]),
        }
    }

    #[must_use]
    pub fn io_buf(&self) -> &IoBuf {
        &self.io_buf
    }

    pub const fn broadcast_local<'b, P>(
        &'a self,
        packet: &'b P,
        center: ChunkPosition,
    ) -> BroadcastLocal<'a, 'b, 'static, P>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        BroadcastLocal {
            packet,
            optional: false,
            compose: *self,
            radius: 0,
            exclude: Cow::Borrowed(&[]),
            center,
        }
    }

    pub fn unicast<P>(&self, packet: &P, id: Packets) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Unicast {
            packet,
            id: id.stream_id,
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
#[derive(Component, Default)]
pub struct IoBuf {
    buffer: RayonLocal<BytesMut>,
    temp_buffer: RayonLocal<BytesMut>,
}

// todo: do we need this many lifetimes? we definitely need 'a and 'b I think
pub struct Broadcast<'a, 'b, 'c, P> {
    packet: &'b P,
    optional: bool,
    compose: Compose<'a>,
    exclude: Cow<'c, [u64]>,
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
            .io_buf
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
            .io_buf
            .multicast_private(self.packet, self.ids, &self.compose)
    }
}

impl<'a, 'b, 'c, P> Broadcast<'a, 'b, 'c, P> {
    #[must_use]
    pub const fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn send(self) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = self
            .compose
            .io_buf
            .encode_packet(self.packet, &self.compose)?;

        self.compose
            .io_buf
            .broadcast_raw(bytes, self.optional, &self.exclude);

        Ok(())
    }

    // todo: discuss whether we should have Into<Cow<'c, [u64]>> or Cow<'c, [u64]>
    // I personally think it makes sense to have `Cow` just so we can have a more flexible API.
    pub fn exclude<'d>(self, exclude: impl Into<Cow<'d, [u64]>>) -> Broadcast<'a, 'b, 'd, P> {
        Broadcast {
            packet: self.packet,
            optional: self.optional,
            compose: self.compose,
            exclude: exclude.into(),
        }
    }
}

pub struct BroadcastLocal<'a, 'b, 'c, P> {
    packet: &'b P,
    compose: Compose<'a>,
    radius: u32,
    center: ChunkPosition,
    optional: bool,
    exclude: Cow<'c, [u64]>,
}

impl<'a, 'b, 'c, P> BroadcastLocal<'a, 'b, 'c, P> {
    #[must_use]
    pub const fn radius(mut self, radius: u32) -> Self {
        self.radius = radius;
        self
    }

    #[must_use]
    pub const fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn send(self) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = self
            .compose
            .io_buf
            .encode_packet(self.packet, &self.compose)?;

        self.compose.io_buf.broadcast_local_raw(
            bytes,
            self.center,
            self.radius,
            self.optional,
            &self.exclude,
        );

        Ok(())
    }

    pub fn exclude<'d>(self, exclude: impl Into<Cow<'d, [u64]>>) -> BroadcastLocal<'a, 'b, 'd, P> {
        BroadcastLocal {
            packet: self.packet,
            compose: self.compose,
            radius: self.radius,
            center: self.center,
            optional: self.optional,
            exclude: exclude.into(),
        }
    }
}

impl IoBuf {
    pub fn split(&mut self) -> impl Iterator<Item = BytesMut> + '_ {
        self.buffer.get_all_mut().iter_mut().map(BytesMut::split)
    }

    fn encode_packet<P>(&self, packet: &P, compose: &Compose) -> anyhow::Result<bytes::Bytes>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let temp_buffer = self.temp_buffer.get_local_raw();
        let temp_buffer = unsafe { &mut *temp_buffer.get() };

        let compressor = compose.compressor.get_local();
        let mut compressor = compressor.borrow_mut();

        let scratch = compose.scratch.get_local();
        let mut scratch = scratch.borrow_mut();

        let result =
            compose
                .encoder()
                .append_packet(packet, temp_buffer, &mut *scratch, &mut compressor)?;

        Ok(result.freeze())
    }

    fn unicast_private<P>(&self, packet: &P, id: u64, compose: &Compose) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = self.encode_packet(packet, compose)?;
        self.unicast_raw(bytes, id);
        Ok(())
    }

    fn multicast_private<P>(&self, packet: &P, ids: &[u64], compose: &Compose) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = self.encode_packet(packet, compose)?;
        self.multicast_raw(bytes, ids);
        Ok(())
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "todo should have something that impl copy/clone"
    )]
    pub fn broadcast_local_raw(
        &self,
        data: bytes::Bytes,
        center: ChunkPosition,
        radius: u32,
        optional: bool,
        exclude: &[u64],
    ) {
        let buffer = self.buffer.get_local_raw();
        let buffer = unsafe { &mut *buffer.get() };

        let to_send = hyperion_proto::BroadcastLocal {
            data,
            taxicab_radius: radius,
            center: Some(center),
            optional,
            exclude: exclude.to_vec(),
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode(buffer)
            .unwrap();
    }

    pub fn broadcast_raw(&self, data: bytes::Bytes, optional: bool, exclude: &[u64]) {
        let buffer = self.buffer.get_local_raw();
        let buffer = unsafe { &mut *buffer.get() };

        let to_send = hyperion_proto::BroadcastGlobal {
            data,
            optional,
            // todo: Right now, we are using `to_vec`.
            // We want to probably allow encoding without allocation in the future.
            // Fortunately, `to_vec` will not require any allocation if the buffer is empty.
            exclude: exclude.to_vec(),
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode(buffer)
            .unwrap();
    }

    pub fn unicast_raw(&self, data: bytes::Bytes, stream: u64) {
        let buffer = self.buffer.get_local_raw();
        let buffer = unsafe { &mut *buffer.get() };

        let to_send = hyperion_proto::Unicast { data, stream };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode(buffer)
            .unwrap();
    }

    pub fn multicast_raw(&self, data: bytes::Bytes, streams: &[u64]) {
        let buffer = self.buffer.get_local_raw();
        let buffer = unsafe { &mut *buffer.get() };

        let to_send = hyperion_proto::Multicast {
            data,
            stream: streams.to_vec(),
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode(buffer)
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use evenio::{
        event::{GlobalEvent, Receiver},
        prelude::World,
    };
    use hyperion_proto as proto;
    use prost::Message;

    use crate::{
        event::Scratches,
        global::Global,
        net::{Compose, Compressors, IoBuf},
    };

    fn rand_bytes(len: usize) -> bytes::Bytes {
        (0..len).map(|_| fastrand::u8(..)).collect()
    }

    fn rand_u64_array(len: usize) -> Vec<u64> {
        (0..len).map(|_| fastrand::u64(..)).collect()
    }

    fn test_handler(_: Receiver<TestEvent>, compose: Compose) {
        let mut left_buf = Vec::new();

        for _ in 0..1 {
            let len = fastrand::usize(..100);
            let taxicab_radius = fastrand::u32(..300);

            let center_x = fastrand::i32(..);
            let center_y = fastrand::i32(..);

            let center = proto::ChunkPosition::new(center_x, center_y);

            let optional = fastrand::bool();

            let len_exclude = fastrand::usize(..100);
            let exclude = rand_u64_array(len_exclude);

            let data = rand_bytes(len);

            // encode using hyperion's definition
            compose.io_buf().broadcast_local_raw(
                data.clone(),
                center,
                taxicab_radius,
                optional,
                &exclude,
            );

            let center = Some(center);

            let left = proto::BroadcastLocal {
                data,
                taxicab_radius,
                center,
                optional,
                exclude,
            };

            let left = proto::ServerToProxyMessage::BroadcastLocal(left);
            let left = proto::ServerToProxy {
                server_to_proxy_message: Some(left),
            };

            // encode using prost's definition which is almost surely correct according to protobuf spec
            left.encode(&mut left_buf).unwrap();

            let right_buf = compose.io_buf();
            let right_buf = right_buf.buffer.get_local_raw();
            let right_buf = unsafe { &mut *right_buf.get() };
            let right_buf = &**right_buf;

            assert_eq!(left_buf, right_buf);
        }
    }

    #[derive(GlobalEvent)]
    struct TestEvent;

    #[test]
    fn test_round_trip() {
        fastrand::seed(7);
        let mut world = World::new();

        // todo: this is a bad way to do things (probably) but I don't really care
        let id = world.spawn();
        world.insert(id, Compressors::default());
        world.insert(id, Scratches::default());
        world.insert(id, Global::default());
        world.insert(id, IoBuf::default());

        world.add_handler(test_handler);

        world.send(TestEvent);
    }
}
