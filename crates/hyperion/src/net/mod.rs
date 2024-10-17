//! All the networking related code.

use std::{
    cell::{Cell, RefCell},
    fmt::Debug,
};

use bumpalo::Bump;
use bytes::BytesMut;
pub use decoder::PacketDecoder;
use derive_more::Deref;
use flecs_ecs::{core::World, macros::Component};
use hyperion_proto::ChunkPosition;
use libdeflater::CompressionLvl;
use prost::Message;

use crate::{
    net::encoder::{append_packet_without_compression, PacketEncoder},
    storage::ThreadLocal,
    system_registry::SystemId,
    Global, PacketBundle, Scratch, Scratches,
};

pub mod decoder;
pub mod encoder;
pub mod packets;
pub mod proxy;

/// The Minecraft protocol version this library currently targets.
pub const PROTOCOL_VERSION: i32 = 763;

// todo: this is one off.. why?
// pub const MAX_PACKET_SIZE: usize = 0x001F_FFFF;
/// The maximum number of bytes that can be sent in a single packet.
pub const MAX_PACKET_SIZE: usize = valence_protocol::MAX_PACKET_SIZE as usize;

/// The stringified name of the Minecraft version this library currently
/// targets.
pub const MINECRAFT_VERSION: &str = "1.20.1";

/// Thread-local [`libdeflater::Compressor`] for encoding packets.
#[derive(Component, Deref)]
pub struct Compressors {
    compressors: ThreadLocal<RefCell<libdeflater::Compressor>>,
}

impl Compressors {
    #[must_use]
    pub(crate) fn new(level: CompressionLvl) -> Self {
        Self {
            compressors: ThreadLocal::new_with(|_| {
                RefCell::new(libdeflater::Compressor::new(level))
            }),
        }
    }
}

/// A reference to a network stream, identified by a stream ID and used to ensure packet order during transmission.
///
/// This struct contains a stream ID that serves as a unique identifier for the network stream and a packet order counter
/// that helps in maintaining the correct sequence of packets being sent through the proxy.
#[derive(Component, Copy, Clone, Debug)]
pub struct NetworkStreamRef {
    /// Unique identifier for the network stream.
    stream_id: u64,
}

impl NetworkStreamRef {
    #[must_use]
    pub(crate) const fn new(stream_id: u64) -> Self {
        Self { stream_id }
    }
}

/// A singleton that can be used to compose and encode packets.
#[derive(Component)]
pub struct Compose {
    compressor: Compressors,
    scratch: Scratches,
    global: Global,
    io_buf: IoBuf,
    pub bump: ThreadLocal<Bump>,
}

impl Compose {
    #[must_use]
    pub fn new(compressor: Compressors, scratch: Scratches, global: Global, io_buf: IoBuf) -> Self {
        Self {
            compressor,
            scratch,
            global,
            io_buf,
            bump: ThreadLocal::new_defaults(),
        }
    }

    #[must_use]
    #[expect(missing_docs)]
    pub const fn global(&self) -> &Global {
        &self.global
    }

    #[expect(missing_docs)]
    pub fn global_mut(&mut self) -> &mut Global {
        &mut self.global
    }

    /// Broadcast globally to all players
    ///
    /// See <https://github.com/andrewgazelka/hyperion-proto/blob/main/src/server_to_proxy.proto#L17-L22>
    pub const fn broadcast<P>(&self, packet: P, system_id: SystemId) -> Broadcast<'_, P>
    where
        P: PacketBundle,
    {
        Broadcast {
            packet,
            optional: false,
            compose: self,
            exclude: 0,
            system_id,
        }
    }

    #[must_use]
    #[expect(missing_docs)]
    pub const fn io_buf(&self) -> &IoBuf {
        &self.io_buf
    }

    #[expect(missing_docs)]
    pub fn io_buf_mut(&mut self) -> &mut IoBuf {
        &mut self.io_buf
    }

    /// Broadcast a packet within a certain region.
    ///
    /// See <https://github.com/andrewgazelka/hyperion-proto/blob/main/src/server_to_proxy.proto#L17-L22>
    pub const fn broadcast_local<'a, 'b, P>(
        &'a self,
        packet: &'b P,
        center: ChunkPosition,
        system_id: SystemId,
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
            system_id,
        }
    }

    /// Send a packet to a single player.
    pub fn unicast<P>(
        &self,
        packet: P,
        stream_id: NetworkStreamRef,
        system_id: SystemId,
        world: &World,
    ) -> anyhow::Result<()>
    where
        P: PacketBundle,
    {
        Unicast {
            packet,
            stream_id,
            compose: self,
            system_id,

            // todo: Should we have this true by default, or is there a better way?
            // Or a better word for no_compress, or should we just use negative field names?
            compress: true,
        }
        .send(world)
    }

    /// Send a packet to a single player without compression.
    pub fn unicast_no_compression<P>(
        &self,
        packet: &P,
        stream_id: NetworkStreamRef,
        system_id: SystemId,
        world: &World,
    ) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Unicast {
            packet,
            stream_id,
            compose: self,
            system_id,
            compress: false,
        }
        .send(world)
    }

    /// Send a packet to multiple players.
    pub fn multicast<P>(
        &self,
        packet: &P,
        ids: &[NetworkStreamRef],
        system_id: SystemId,
        world: &World,
    ) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        Multicast {
            packet,
            compose: self,
            system_id,
            ids: unsafe { core::slice::from_raw_parts(ids.as_ptr().cast(), ids.len()) },
        }
        .send(world)
    }

    #[must_use]
    pub(crate) fn encoder(&self) -> PacketEncoder {
        let threshold = self.global.shared.compression_threshold;
        PacketEncoder::new(threshold)
    }

    /// Obtain a thread-local scratch buffer.
    #[must_use]
    pub fn scratch(&self, world: &World) -> &RefCell<Scratch> {
        self.scratch.get(world)
    }

    /// Obtain a thread-local [`libdeflater::Compressor`]
    #[must_use]
    pub fn compressor(&self, world: &World) -> &RefCell<libdeflater::Compressor> {
        self.compressor.get(world)
    }
}

/// This is useful for the ECS, so we can use Single<&mut Broadcast> instead of having to use a marker struct
#[derive(Component, Default)]
pub struct IoBuf {
    buffer: ThreadLocal<RefCell<BytesMut>>,
    // system_on: ThreadLocal<Cell<u32>>,
    // broadcast_buffer: ThreadLocal<RefCell<BytesMut>>,
    temp_buffer: ThreadLocal<RefCell<BytesMut>>,
    idx: ThreadLocal<Cell<u16>>,
}

impl IoBuf {
    pub fn fetch_add_idx(&self, world: &World) -> u16 {
        let cell = self.idx.get(world);
        let result = cell.get();
        cell.set(result + 1);
        result
    }

    pub fn order_id(&self, system_id: SystemId, world: &World) -> u32 {
        u32::from(system_id.id()) << 16 | u32::from(self.fetch_add_idx(world))
    }
}

/// A broadcast builder
#[must_use]
pub struct Broadcast<'a, P> {
    packet: P,
    optional: bool,
    compose: &'a Compose,
    exclude: u64,
    system_id: SystemId,
}

/// A unicast builder
#[must_use]
struct Unicast<'a, P> {
    packet: P,
    stream_id: NetworkStreamRef,
    compose: &'a Compose,
    compress: bool,
    system_id: SystemId,
}

impl<P> Unicast<'_, P>
where
    P: PacketBundle,
{
    fn send(self, world: &World) -> anyhow::Result<()> {
        self.compose.io_buf.unicast_private(
            self.packet,
            self.stream_id,
            self.compose,
            self.compress,
            self.system_id,
            world,
        )
    }
}

struct Multicast<'a, 'b, P> {
    packet: &'b P,
    ids: &'a [u64],
    compose: &'a Compose,
    system_id: SystemId,
}

impl<P> Multicast<'_, '_, P>
where
    P: valence_protocol::Packet + valence_protocol::Encode,
{
    fn send(&self, world: &World) -> anyhow::Result<()> {
        self.compose.io_buf.multicast_private(
            self.packet,
            self.ids,
            self.compose,
            self.system_id,
            world,
        )
    }
}

impl<P> Broadcast<'_, P> {
    /// If the packet is optional and can be dropped. An example is movement packets.
    pub const fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// Send the packet to all players.
    pub fn send(self, world: &World) -> anyhow::Result<()>
    where
        P: PacketBundle,
    {
        let bytes = self
            .compose
            .io_buf
            .encode_packet(self.packet, self.compose, world)?;

        self.compose.io_buf.broadcast_raw(
            bytes,
            self.optional,
            self.exclude,
            self.system_id,
            world,
        );

        Ok(())
    }

    /// Exclude a certain player from the broadcast. This can only be called once.
    pub fn exclude(self, exclude: NetworkStreamRef) -> Self {
        Broadcast {
            packet: self.packet,
            optional: self.optional,
            compose: self.compose,
            system_id: self.system_id,
            exclude: exclude.stream_id,
        }
    }
}

#[must_use]
#[expect(missing_docs)]
pub struct BroadcastLocal<'a, 'b, P> {
    packet: &'b P,
    compose: &'a Compose,
    radius: u32,
    center: ChunkPosition,
    optional: bool,
    exclude: u64,
    system_id: SystemId,
}

impl<P> BroadcastLocal<'_, '_, P> {
    /// The radius of the broadcast. The radius is measured by Chebyshev distance
    pub const fn radius(mut self, radius: u32) -> Self {
        self.radius = radius;
        self
    }

    /// If the packet is optional and can be dropped. An example is movement packets.
    pub const fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// Send the packet
    pub fn send(self, world: &World) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = self
            .compose
            .io_buf
            .encode_packet(self.packet, self.compose, world)?;

        self.compose.io_buf.broadcast_local_raw(
            bytes,
            self.center,
            self.radius,
            self.optional,
            self.exclude,
            self.system_id,
            world,
        );

        Ok(())
    }

    /// Exclude a certain player from the broadcast. This can only be called once.
    pub const fn exclude(self, exclude: &NetworkStreamRef) -> Self {
        BroadcastLocal {
            packet: self.packet,
            compose: self.compose,
            radius: self.radius,
            center: self.center,
            optional: self.optional,
            exclude: exclude.stream_id,
            system_id: self.system_id,
        }
    }
}

impl IoBuf {
    /// Returns an iterator over the result of splitting the buffer into packets with [`BytesMut::split`].
    pub fn reset_and_split(&mut self) -> impl Iterator<Item = BytesMut> + '_ {
        // reset idx
        for elem in &mut self.idx {
            elem.set(0);
        }

        self.buffer
            .iter_mut()
            .map(|x| x.borrow_mut())
            .map(|mut x| x.split())
    }

    fn encode_packet<P>(
        &self,
        packet: P,
        compose: &Compose,
        world: &World,
    ) -> anyhow::Result<bytes::Bytes>
    where
        P: PacketBundle,
    {
        let temp_buffer = self.temp_buffer.get(world);
        let temp_buffer = &mut *temp_buffer.borrow_mut();

        let compressor = compose.compressor(world);
        let mut compressor = compressor.borrow_mut();

        let scratch = compose.scratch.get(world);
        let mut scratch = scratch.borrow_mut();

        let result =
            compose
                .encoder()
                .append_packet(packet, temp_buffer, &mut *scratch, &mut compressor)?;

        Ok(result.freeze())
    }

    fn encode_packet_no_compression<P>(
        &self,
        packet: P,
        world: &World,
    ) -> anyhow::Result<bytes::Bytes>
    where
        P: PacketBundle,
    {
        let temp_buffer = self.temp_buffer.get(world);
        let temp_buffer = &mut *temp_buffer.borrow_mut();

        let result = append_packet_without_compression(packet, temp_buffer)?;

        Ok(result.freeze())
    }

    fn unicast_private<P>(
        &self,
        packet: P,
        id: NetworkStreamRef,
        compose: &Compose,
        compress: bool,
        system_id: SystemId,
        world: &World,
    ) -> anyhow::Result<()>
    where
        P: PacketBundle,
    {
        let bytes = if compress {
            self.encode_packet(packet, compose, world)?
        } else {
            self.encode_packet_no_compression(packet, world)?
        };

        self.unicast_raw(bytes, id, system_id, world);
        Ok(())
    }

    fn multicast_private<P>(
        &self,
        packet: &P,
        ids: &[u64],
        compose: &Compose,
        system_id: SystemId,
        world: &World,
    ) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + valence_protocol::Encode,
    {
        let bytes = self.encode_packet(packet, compose, world)?;
        self.multicast_raw(bytes, ids, system_id, world);
        Ok(())
    }

    #[expect(clippy::too_many_arguments, reason = "todo")]
    fn broadcast_local_raw(
        &self,
        data: bytes::Bytes,
        center: ChunkPosition,
        radius: u32,
        optional: bool,
        exclude: u64,
        system_id: SystemId,
        world: &World,
    ) {
        let buffer = self.buffer.get(world);
        let buffer = &mut *buffer.borrow_mut();

        let order = self.order_id(system_id, world);

        let to_send = hyperion_proto::BroadcastLocal {
            data,
            taxicab_radius: radius,
            center: Some(center),
            optional,
            exclude,
            order,
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode_length_delimited(buffer)
            .unwrap();
    }

    pub(crate) fn broadcast_raw(
        &self,
        data: bytes::Bytes,
        optional: bool,
        exclude: u64,
        system_id: SystemId,
        world: &World,
    ) {
        let buffer = self.buffer.get(world);
        let buffer = &mut *buffer.borrow_mut();

        let order = u32::from(system_id.id()) << 16;

        let to_send = hyperion_proto::BroadcastGlobal {
            data,
            optional,
            // todo: Right now, we are using `to_vec`.
            // We want to probably allow encoding without allocation in the future.
            // Fortunately, `to_vec` will not require any allocation if the buffer is empty.
            exclude,
            order,
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode_length_delimited(buffer)
            .unwrap();
    }

    pub(crate) fn unicast_raw(
        &self,
        data: bytes::Bytes,
        stream: NetworkStreamRef,
        system_id: SystemId,
        world: &World,
    ) {
        let buffer = self.buffer.get(world);
        let buffer = &mut *buffer.borrow_mut();

        let order = self.order_id(system_id, world);

        let to_send = hyperion_proto::Unicast {
            data,
            stream: stream.stream_id,
            order,
        };

        let to_send = hyperion_proto::ServerToProxy::from(to_send);
        to_send.encode_length_delimited(buffer).unwrap();
    }

    pub(crate) fn multicast_raw(
        &self,
        data: bytes::Bytes,
        streams: &[u64],
        system_id: SystemId,
        world: &World,
    ) {
        let buffer = self.buffer.get(world);
        let buffer = &mut *buffer.borrow_mut();

        let order = self.order_id(system_id, world);

        let to_send = hyperion_proto::Multicast {
            data,
            stream: streams.to_vec(),
            order,
        };

        hyperion_proto::ServerToProxy::from(to_send)
            .encode_length_delimited(buffer)
            .unwrap();
    }

    pub(crate) fn set_receive_broadcasts(&self, stream: NetworkStreamRef, world: &World) {
        let buffer = self.buffer.get(world);
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
