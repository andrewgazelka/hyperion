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

#[derive(Copy, Clone)]
pub enum PacketNecessity {
    Required,
    Droppable {
        #[expect(
            dead_code,
            reason = "this is not used, but we plan to use it in the future"
        )]
        prioritize_location: Vec2,
    },
}

#[derive(Copy, Clone)]
#[expect(
    dead_code,
    reason = "this is not used, but we plan to use it in the future"
)]
pub struct PacketMetadata {
    pub necessity: PacketNecessity,
    pub exclude_player: Option<Uuid>,
}

impl PacketMetadata {
    pub const DROPPABLE: Self = Self {
        necessity: PacketNecessity::Droppable {
            prioritize_location: Vec2::new(0.0, 0.0),
        },
        exclude_player: None,
    };
    #[expect(
        dead_code,
        reason = "this is not used, but we plan to use it in the future"
    )]
    pub const REQUIRED: Self = Self {
        necessity: PacketNecessity::Required,
        exclude_player: None,
    };
}

/// Packet which should not be dropped
#[expect(
    dead_code,
    reason = "this is not used, but we plan to use it in the future"
)]
pub struct NecessaryPacket {
    pub exclude_player: Option<Uuid>,
    pub offset: usize,
    pub len: usize,
}

/// Packet which may be dropped
#[expect(
    dead_code,
    reason = "this is not used, but we plan to use it in the future"
)]
pub struct DroppablePacket {
    pub prioritize_location: Vec2,
    pub exclude_player: Option<Uuid>,
    pub offset: usize,
    pub len: usize,
}

#[derive(Component)]
pub struct Broadcast {
    rayon_local: RayonLocal<Cell<PacketEncoder>>,
}

impl Broadcast {
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

impl Broadcast {
    #[expect(
        unused_variables,
        reason = "`metadata` is planned to be used in the future to allow droppable packets with \
                  a priority"
    )]
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

    pub fn get_round_robin(&mut self) -> &mut PacketEncoder {
        let local = self.rayon_local.get_local_round_robin();
        local.get_mut()
    }

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
}
