//! Defines a singleton that is used to broadcast packets to all players.

// https://stackoverflow.com/a/61681112/4889030
// https://matklad.github.io/2020/10/03/fast-thread-locals-in-rust.html
use std::cell::Cell;

use evenio::prelude::Component;
use rayon::iter::IntoParallelRefMutIterator;
pub use rayon::iter::ParallelIterator;
use rayon_local::RayonLocal;
use tracing::trace;
use uuid::Uuid;
use valence_protocol::{math::Vec2, CompressionThreshold, Encode, Packet, PacketEncoder};

/// A definition of whether a packet is always required or can be dropped.
///
/// This is useful when a player has a limited amount of bandwidth and we want to prioritize
/// sending packets to the player.
#[derive(Copy, Clone)]
pub enum PacketNecessity {
    /// The packet is always required and cannot be dropped. An example would be an entity spawn packet.
    Required,

    /// The packet is optional and can be dropped. An example would be a player position packet, entity movement packet, etc.
    Droppable {
        /// The location to prioritize the packet at. If this is an entity movement packet, this is the location of the entity.
        /// This will mean
        /// that the packet is more likely to be sent to players near to this location if their bandwidth is limited.
        #[expect(
            dead_code,
            reason = "this is not used, but we plan to use it in the future"
        )]
        prioritize_location: Vec2,
    },
}

/// Metadata for determining how to send a packet.
#[derive(Copy, Clone)]
#[expect(
    dead_code,
    reason = "this is not used, but we plan to use it in the future"
)]
pub struct PacketMetadata {
    /// Determines whether the packet is required or optional.
    pub necessity: PacketNecessity,
    /// The player to exclude from the packet.
    /// For instance, if a player is broadcasting their own position,
    /// they should not be included in the broadcast of that packet.
    ///
    /// todo: implement `exclude_player` and use a more efficient option (perhaps a global packet bitmask)
    pub exclude_player: Option<Uuid>,
}

impl PacketMetadata {
    /// The server can drop the packet (with no prioritization of location).
    #[expect(
        dead_code,
        reason = "this is not used, but we plan to use it in the future"
    )]
    pub const DROPPABLE: Self = Self {
        necessity: PacketNecessity::Droppable {
            prioritize_location: Vec2::new(0.0, 0.0),
        },
        exclude_player: None,
    };
    /// The packet is required.
    #[expect(
        dead_code,
        reason = "this is not used, but we plan to use it in the future"
    )]
    pub const REQUIRED: Self = Self {
        necessity: PacketNecessity::Required,
        exclude_player: None,
    };
}

/// See [`crate::singleton::broadcast`].
#[derive(Component)]
pub struct BroadcastBuf {
    /// We want to be able to write to a [`PacketEncoder`] from multiple threads without locking.
    /// In order to do this, we use a [`RayonLocal`] to store a reference to the [`PacketEncoder`]
    /// for each thread.
    rayon_local: RayonLocal<Cell<PacketEncoder>>,
}

impl BroadcastBuf {
    /// Creates a new [`Self`] with the given compression level.
    pub fn new(compression_level: CompressionThreshold) -> Self {
        Self {
            rayon_local: RayonLocal::init_with(|| {
                let mut encoder = PacketEncoder::default();
                encoder.set_compression(compression_level);
                Cell::new(encoder)
            }),
        }
    }
}

pub struct AppendOnlyEncoder<'a> {
    encoder: &'a mut PacketEncoder,
}

impl<'a> AppendOnlyEncoder<'a> {
    fn new(encoder: &'a mut PacketEncoder) -> Self {
        Self { encoder }
    }

    pub fn append_raw(&mut self, data: &[u8]) {
        self.encoder.append_bytes(data);
    }

    pub fn append_packet<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        println!("appending to broadcast packet {}", P::NAME);
        self.encoder.append_packet(pkt)
    }
}

impl BroadcastBuf {
    #[expect(
        unused_variables,
        reason = "`metadata` is planned to be used in the future to allow droppable packets with \
                  a priority"
    )]

    /// Appends a packet to the buffer to be broadcast to all players.
    pub fn append<P: Packet + Encode>(
        &self,
        packet: &P,
        metadata: PacketMetadata,
    ) -> anyhow::Result<()> {
        let local = self.rayon_local.get_rayon_local();
        let mut encoder = local.take();

        trace!("append broadcast packet {} {}", P::ID, P::NAME);

        let result = encoder.append_packet(packet);
        local.set(encoder);
        result
    }

    /// Returns a reference to the [`PacketEncoder`] usually local to a rayon thread based on a
    /// round robin policy.
    /// This is so that packets can evenly be spread out across threads.
    pub fn get_round_robin(&mut self) -> AppendOnlyEncoder {
        let local = self.rayon_local.get_local_round_robin();
        let local = local.get_mut();
        AppendOnlyEncoder::new(local)
    }

    /// Drain all buffers in parallel. This is useful for sending the buffers to the actual players.
    pub fn par_drain<F>(&mut self, f: F)
    where
        F: Fn(bytes::Bytes) + Sync,
    {
        self.rayon_local
            .get_all_locals()
            .par_iter_mut()
            .for_each(|encoder| {
                let encoder = encoder.get_mut();
                let bytes = encoder.take().freeze();
                if bytes.is_empty() {
                    return;
                }
                f(bytes);
            });
    }

    pub fn drain(&mut self, mut f: impl FnMut(bytes::Bytes)) {
        self.rayon_local
            .get_all_locals()
            .iter_mut()
            .for_each(|encoder| {
                let encoder = encoder.get_mut();
                let bytes = encoder.take().freeze();
                if bytes.is_empty() {
                    return;
                }
                f(bytes);
            });
    }
}
