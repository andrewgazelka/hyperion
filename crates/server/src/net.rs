//! All the networking related code.

use std::{cell::RefCell, fmt::Debug, ops::Deref, sync::atomic::AtomicU64};

use bytes::BytesMut;
pub use decoder::PacketDecoder;
use flecs_ecs::macros::Component;
use hyperion_proto::ChunkPosition;
use libdeflater::CompressionLvl;
use prost::Message;
use slotmap::KeyData;
use thread_local::ThreadLocal;

use crate::{
    global::Global,
    net::encoder::{append_packet_without_compression, PacketEncoder},
    Scratch, Scratches,
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

#[derive(Component)]
pub struct Compressors {
    compressors: ThreadLocal<RefCell<libdeflater::Compressor>>,
    level: CompressionLvl,
}

impl Compressors {
    #[must_use]
    pub const fn new(level: CompressionLvl) -> Self {
        Self {
            compressors: ThreadLocal::new(),
            level,
        }
    }
}

impl Deref for Compressors {
    type Target = RefCell<libdeflater::Compressor>;

    fn deref(&self) -> &Self::Target {
        self.compressors
            .get_or(|| libdeflater::Compressor::new(self.level).into())
    }
}

impl Default for Compressors {
    fn default() -> Self {
        Self::new(CompressionLvl::default())
    }
}

#[derive(Component)]
pub struct IoRef {
    stream_id: u64,

    /// This starts at 0. Every single packet we encode, we increment this by 1,
    /// and we wrap it around if we ever get to the maximum value.
    /// This is used so that the proxy will always send these packets in order.
    /// It's sent as part of the protocol.  
    packet_order: AtomicU64,
}

impl Debug for IoRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stream_id = KeyData::from_ffi(self.stream_id);

        f.debug_struct("StreamId")
            .field("stream_id", &stream_id)
            .field("packet_on", &self.packet_order)
            .finish()
    }
}

impl IoRef {
    #[must_use]
    pub const fn new(stream_id: u64) -> Self {
        Self {
            stream_id,
            packet_order: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub const fn stream(&self) -> u64 {
        self.stream_id
    }
}

#[derive(Component)]
pub struct Compose {
    // todo: make these other things private for safety
    compressor: Compressors,
    scratch: Scratches,
    global: Global,
    io_buf: IoBuf,
}

impl Compose {
    #[must_use]
    pub const fn new(
        compressor: Compressors,
        scratch: Scratches,
        global: Global,
        io_buf: IoBuf,
    ) -> Self {
        Self {
            compressor,
            scratch,
            global,
            io_buf,
        }
    }

    #[must_use]
    pub const fn global(&self) -> &Global {
        &self.global
    }

    pub fn global_mut(&mut self) -> &mut Global {
        &mut self.global
    }

    /// Broadcast globally to all players
    ///
    /// See <https://github.com/andrewgazelka/hyperion-proto/blob/main/src/server_to_proxy.proto#L17-L22>
    pub const fn broadcast<'a, 'b, P>(&'a self, packet: &'b P) -> Broadcast<'a, 'b, P>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Broadcast {
            packet,
            optional: false,
            compose: self,
            exclude: 0,
        }
    }

    #[must_use]
    pub const fn io_buf(&self) -> &IoBuf {
        &self.io_buf
    }

    pub fn io_buf_mut(&mut self) -> &mut IoBuf {
        &mut self.io_buf
    }

    pub const fn broadcast_local<'a, 'b, P>(
        &'a self,
        packet: &'b P,
        center: ChunkPosition,
    ) -> BroadcastLocal<'a, 'b, P>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        BroadcastLocal {
            packet,
            optional: false,
            compose: self,
            radius: 0,
            exclude: 0,
            center,
        }
    }

    pub fn unicast<P>(&self, packet: &P, stream_id: &IoRef) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Unicast {
            packet,
            stream_id,
            compose: self,

            // todo: Should we have this true by default, or is there a better way?
            // Or a better word for no_compress, or should we just use negative field names?
            compress: true,
        }
        .send()
    }

    pub fn unicast_no_compression<P>(&self, packet: &P, stream_id: &IoRef) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Unicast {
            packet,
            stream_id,
            compose: self,
            compress: false,
        }
        .send()
    }

    pub fn multicast<P>(&self, packet: &P, ids: &[IoRef]) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Multicast {
            packet,
            compose: self,
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
        self.scratch.get_or_default()
    }

    #[must_use]
    pub fn compressor(&self) -> &RefCell<libdeflater::Compressor> {
        &self.compressor
    }
}

/// This is useful for the ECS, so we can use Single<&mut Broadcast> instead of having to use a marker struct
#[derive(Component, Default)]
pub struct IoBuf {
    buffer: ThreadLocal<RefCell<BytesMut>>,
    temp_buffer: ThreadLocal<RefCell<BytesMut>>,
}

// todo: do we need this many lifetimes? we definitely need 'a and 'b I think
#[must_use]
pub struct Broadcast<'a, 'b, P> {
    packet: &'b P,
    optional: bool,
    compose: &'a Compose,
    exclude: u64,
}

#[must_use]
struct Unicast<'a, 'b, 'c, P> {
    packet: &'b P,
    stream_id: &'c IoRef,
    compose: &'a Compose,
    compress: bool,
}

impl<'a, 'b, 'c, P> Unicast<'a, 'b, 'c, P>
where
    P: valence_protocol::Packet + valence_protocol::Encode,
{
    fn send(&self) -> anyhow::Result<()> {
        self.compose.io_buf.unicast_private(
            self.packet,
            self.stream_id,
            self.compose,
            self.compress,
        )
    }
}

struct Multicast<'a, 'b, P> {
    packet: &'b P,
    ids: &'a [u64],
    compose: &'a Compose,
}

impl<'a, 'b, P> Multicast<'a, 'b, P>
where
    P: valence_protocol::Packet + valence_protocol::Encode,
{
    fn send(&self) -> anyhow::Result<()> {
        self.compose
            .io_buf
            .multicast_private(self.packet, self.ids, self.compose)
    }
}

impl<'a, 'b, P> Broadcast<'a, 'b, P> {
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
            .encode_packet(self.packet, self.compose)?;

        self.compose
            .io_buf
            .broadcast_raw(bytes, self.optional, self.exclude);

        Ok(())
    }

    pub const fn exclude(self, exclude: &IoRef) -> Self {
        Broadcast {
            packet: self.packet,
            optional: self.optional,
            compose: self.compose,
            exclude: exclude.stream_id,
        }
    }
}

#[must_use]
pub struct BroadcastLocal<'a, 'b, P> {
    packet: &'b P,
    compose: &'a Compose,
    radius: u32,
    center: ChunkPosition,
    optional: bool,
    exclude: u64,
}

impl<'a, 'b, P> BroadcastLocal<'a, 'b, P> {
    pub const fn radius(mut self, radius: u32) -> Self {
        self.radius = radius;
        self
    }

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
            .encode_packet(self.packet, self.compose)?;

        self.compose.io_buf.broadcast_local_raw(
            bytes,
            self.center,
            self.radius,
            self.optional,
            self.exclude,
        );

        Ok(())
    }

    pub const fn exclude(self, exclude: &IoRef) -> Self {
        BroadcastLocal {
            packet: self.packet,
            compose: self.compose,
            radius: self.radius,
            center: self.center,
            optional: self.optional,
            exclude: exclude.stream_id,
        }
    }
}

impl IoBuf {
    pub fn split(&mut self) -> impl Iterator<Item = BytesMut> + '_ {
        self.buffer
            .iter_mut()
            .map(|x| x.borrow_mut())
            .map(|mut x| x.split())
    }

    fn encode_packet<P>(&self, packet: &P, compose: &Compose) -> anyhow::Result<bytes::Bytes>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let temp_buffer = self.temp_buffer.get_or_default();
        let temp_buffer = &mut *temp_buffer.borrow_mut();

        let compressor = compose.compressor();
        let mut compressor = compressor.borrow_mut();

        let scratch = compose.scratch.get_or_default();
        let mut scratch = scratch.borrow_mut();

        let result =
            compose
                .encoder()
                .append_packet(packet, temp_buffer, &mut *scratch, &mut compressor)?;

        Ok(result.freeze())
    }

    fn encode_packet_no_compression<P>(&self, packet: &P) -> anyhow::Result<bytes::Bytes>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let temp_buffer = self.temp_buffer.get_or_default();
        let temp_buffer = &mut *temp_buffer.borrow_mut();

        let result = append_packet_without_compression(packet, temp_buffer)?;

        Ok(result.freeze())
    }

    fn unicast_private<P>(
        &self,
        packet: &P,
        id: &IoRef,
        compose: &Compose,
        compress: bool,
    ) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = if compress {
            self.encode_packet(packet, compose)?
        } else {
            self.encode_packet_no_compression(packet)?
        };

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
        exclude: u64,
    ) {
        let buffer = self.buffer.get_or_default();
        let buffer = &mut *buffer.borrow_mut();

        let to_send = hyperion_proto::BroadcastLocal {
            data,
            taxicab_radius: radius,
            center: Some(center),
            optional,
            exclude,
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode_length_delimited(buffer)
            .unwrap();
    }

    pub fn broadcast_raw(&self, data: bytes::Bytes, optional: bool, exclude: u64) {
        let buffer = self.buffer.get_or_default();
        let buffer = &mut *buffer.borrow_mut();

        let to_send = hyperion_proto::BroadcastGlobal {
            data,
            optional,
            // todo: Right now, we are using `to_vec`.
            // We want to probably allow encoding without allocation in the future.
            // Fortunately, `to_vec` will not require any allocation if the buffer is empty.
            exclude,
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode_length_delimited(buffer)
            .unwrap();
    }

    pub fn unicast_raw(&self, data: bytes::Bytes, stream: &IoRef) {
        let buffer = self.buffer.get_or_default();
        let buffer = &mut *buffer.borrow_mut();

        let to_send = hyperion_proto::Unicast {
            data,
            stream: stream.stream_id,
            order: stream
                .packet_order
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        };

        let to_send = hyperion_proto::ServerToProxy::from(to_send);
        to_send.encode_length_delimited(buffer).unwrap();
    }

    pub fn multicast_raw(&self, data: bytes::Bytes, streams: &[u64]) {
        let buffer = self.buffer.get_or_default();
        let buffer = &mut *buffer.borrow_mut();

        let to_send = hyperion_proto::Multicast {
            data,
            stream: streams.to_vec(),
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode_length_delimited(buffer)
            .unwrap();
    }

    pub fn set_receive_broadcasts(&self, stream: &IoRef) {
        let buffer = self.buffer.get_or_default();
        let buffer = &mut *buffer.borrow_mut();

        let to_send = hyperion_proto::SetReceiveBroadcasts {
            stream: stream.stream_id,
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode_length_delimited(buffer)
            .unwrap();
    }
}

// #[cfg(test)]
// mod tests {
//     use evenio::{
//         event::{GlobalEvent, Receiver},
//         prelude::World,
//     };
//     use hyperion_proto as proto;
//     use prost::Message;
//
//     use crate::{
//         event::Scratches,
//         global::Global,
//         net::{Compose, Compressors, IoBuf},
//     };
//
//     fn rand_bytes(len: usize) -> bytes::Bytes {
//         (0..len).map(|_| fastrand::u8(..)).collect()
//     }
//
//     fn rand_u64_array(len: usize) -> Vec<u64> {
//         (0..len).map(|_| fastrand::u64(..)).collect()
//     }
//
//     fn test_handler(_: Receiver<TestEvent>, compose: Compose) {
//         let mut left_buf = Vec::new();
//
//         for _ in 0..1 {
//             let len = fastrand::usize(..100);
//             let taxicab_radius = fastrand::u32(..300);
//
//             let center_x = fastrand::i32(..);
//             let center_y = fastrand::i32(..);
//
//             let center = proto::ChunkPosition::new(center_x, center_y);
//
//             let optional = fastrand::bool();
//
//             let len_exclude = fastrand::usize(..100);
//             let exclude = rand_u64_array(len_exclude);
//
//             let data = rand_bytes(len);
//
//             // encode using hyperion's definition
//             compose.io_buf().broadcast_local_raw(
//                 data.clone(),
//                 center,
//                 taxicab_radius,
//                 optional,
//                 &exclude,
//             );
//
//             let center = Some(center);
//
//             let left = proto::BroadcastLocal {
//                 data,
//                 taxicab_radius,
//                 center,
//                 optional,
//                 exclude,
//             };
//
//             let left = proto::ServerToProxyMessage::BroadcastLocal(left);
//             let left = proto::ServerToProxy {
//                 server_to_proxy_message: Some(left),
//             };
//
//             // encode using prost's definition which is almost surely correct according to protobuf spec
//             left.encode_length_delimited(&mut left_buf).unwrap();
//
//             let right_buf = compose.io_buf();
//             let right_buf = right_buf.buffer.get_local();
//             let right_buf = right_buf.borrow_mut();
//             let right_buf = &**right_buf;
//
//             assert_eq!(left_buf, right_buf);
//         }
//     }
//
//     #[derive(GlobalEvent)]
//     struct TestEvent;
//
//     #[test]
//     fn test_round_trip() {
//         fastrand::seed(7);
//         let mut world = World::new();
//
//         // todo: this is a bad way to do things (probably) but I don't really care
//         let id = world.spawn();
//         world.insert(id, Compressors::default());
//         world.insert(id, Scratches::default());
//         world.insert(id, Global::default());
//         world.insert(id, IoBuf::default());
//
//         world.add_handler(test_handler);
//
//         world.send(TestEvent);
//     }
// }
