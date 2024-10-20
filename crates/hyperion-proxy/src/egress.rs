use std::sync::{atomic::Ordering, Arc};

use arc_swap::ArcSwap;
use bvh::{Aabb, Bvh, Data, Point};
use glam::I16Vec2;
use hyperion_proto::{ChunkPosition, ServerToProxyMessage};
use slotmap::KeyData;
use tracing::{debug, error, instrument, warn};

use crate::{
    cache::GlobalExclusionsManager,
    data::{OrderedBytes, PlayerId, PlayerRegistry},
};

#[derive(Default, Debug)]
struct PositionData {
    streams: Vec<u64>,
    positions: Vec<ChunkPosition>,
}

#[derive(Default)]
pub struct Egress {
    registry: Arc<PlayerRegistry>,
    positions: ArcSwap<PositionData>,
}

pub struct BroadcastLocalInstruction {
    pub order: u32,
    pub bvh: Arc<Bvh<bytes::Bytes>>,
}

impl Egress {
    #[must_use]
    #[instrument]
    pub fn new(registry: Arc<PlayerRegistry>) -> Self {
        Self {
            registry,
            positions: ArcSwap::default(),
        }
    }

    pub fn handle_packet(self: &Arc<Self>, packet: ServerToProxyMessage) {
        match packet {
            ServerToProxyMessage::UpdatePlayerChunkPositions(pkt) => {
                self.handle_update_player_chunk_positions(pkt);
            }
            ServerToProxyMessage::Multicast(pkt) => {
                self.handle_multicast(pkt);
            }
            ServerToProxyMessage::Unicast(pkt) => {
                self.handle_unicast(pkt);
            }
            ServerToProxyMessage::SetReceiveBroadcasts(pkt) => {
                self.handle_set_receive_broadcasts(pkt);
            }
            ServerToProxyMessage::BroadcastLocal(..)
            | ServerToProxyMessage::BroadcastGlobal(..)
            | ServerToProxyMessage::Flush(..) => {}
        }
    }

    #[instrument(skip(self, pkt))]
    pub fn handle_update_player_chunk_positions(
        &self,
        pkt: hyperion_proto::UpdatePlayerChunkPositions,
    ) {
        let position_data = PositionData {
            streams: pkt.stream,
            positions: pkt.positions,
        };

        self.positions.store(Arc::new(position_data));
    }

    #[instrument(skip(self, pkt, exclusions), level = "trace")]
    pub fn handle_broadcast_global(
        &self,
        pkt: hyperion_proto::BroadcastGlobal,
        exclusions: GlobalExclusionsManager,
    ) {
        let data = pkt.data;

        let players = self.registry.read().unwrap();

        let exclusions = Arc::new(exclusions);

        // imo it makes sense to read once... it is a fast loop
        #[allow(clippy::significant_drop_in_scrutinee)]
        for player in players.values() {
            if !player.can_receive_broadcasts.load(Ordering::Relaxed) {
                continue;
            }

            let to_send =
                OrderedBytes::with_exclusions(pkt.order, data.clone(), exclusions.clone());

            if let Err(e) = player.writer.try_send(to_send) {
                debug!("Failed to send data to player: {:?}", e);
            }
        }
    }

    #[instrument(skip_all)]
    pub fn handle_flush(&self) {
        let players = self.registry.read().unwrap();

        for player in players.values() {
            if let Err(e) = player.writer.try_send(OrderedBytes::FLUSH) {
                warn!("Failed to send data to player: {:?}", e);
            }
        }
    }

    pub fn handle_broadcast_local(self: Arc<Self>, instruction: BroadcastLocalInstruction) {
        let order = instruction.order;
        let bvh = instruction.bvh;

        // we are spawning because it is rather intensive to call get_in_slices on a bvh
        #[allow(clippy::significant_drop_tightening)]
        tokio::spawn(async move {
            const RADIUS: i16 = 8;

            let positions = self.positions.load();
            let players = self.registry.read().unwrap();

            let mut byte_slice_total = 0;
            let total_players = players.len();

            for (&id, &position) in positions.streams.iter().zip(positions.positions.iter()) {
                let id = KeyData::from_ffi(id);
                let id = PlayerId::from(id);

                let Some(player) = players.get(id) else {
                    // expected to still happen infrequently
                    debug!("Player not found for id {id:?}");
                    continue;
                };

                if !player.can_receive_broadcasts.load(Ordering::Relaxed) {
                    continue;
                }

                let position = I16Vec2::new(position.x as i16, position.z as i16);
                let min = position - I16Vec2::splat(RADIUS);
                let max = position + I16Vec2::splat(RADIUS);

                let aabb = Aabb::new(min, max);
                let byte_slices = bvh.get_in_slices_bytes(aabb);
                byte_slice_total += byte_slices.len();

                for data in byte_slices {
                    if let Err(e) = player.writer.try_send(OrderedBytes {
                        order,
                        data,
                        exclusions: None,
                    }) {
                        debug!("Failed to send data to player: {:?}", e);
                    }
                }
            }

            let avg = byte_slice_total as f32 / total_players as f32;
            println!("average byte slice size: {avg:.2} elems");
        });
    }

    #[instrument(skip(self, pkt))]
    pub fn handle_multicast(&self, pkt: hyperion_proto::Multicast) {
        let players = self.registry.read().unwrap();
        let data = pkt.data;

        // todo: ArrayVec might overflow
        let players = pkt
            .stream
            .iter()
            .map(|id| KeyData::from_ffi(*id))
            .map(PlayerId::from)
            .filter_map(|id| players.get(id));

        for player in players {
            let to_send = OrderedBytes::no_order(data.clone());
            // todo: handle error; kick player if cannot send (buffer full)
            if let Err(e) = player.writer.try_send(to_send) {
                debug!("Failed to send data to player: {:?}", e);
            }
        }
    }

    // #[instrument(skip(self, pkt))]
    pub fn handle_unicast(&self, pkt: hyperion_proto::Unicast) {
        let data = pkt.data;

        let ordered = OrderedBytes {
            order: pkt.order,
            data,
            exclusions: None,
        };

        let id = pkt.stream;

        let id = KeyData::from_ffi(id);
        let id = PlayerId::from(id);

        let players = self.registry.read().unwrap();
        let Some(player) = players.get(id) else {
            // expected to still happen infrequently
            debug!("Player not found for id {id:?}");
            return;
        };

        // todo: handle error; kick player if cannot send (buffer full)
        if let Err(e) = player.writer.try_send(ordered) {
            debug!("Failed to send data to player: {:?}", e);
        }

        drop(players);
    }

    pub fn handle_set_receive_broadcasts(&self, pkt: hyperion_proto::SetReceiveBroadcasts) {
        let registry = self.registry.clone();
        let players = registry.read().unwrap();
        let stream = pkt.stream;
        let stream = KeyData::from_ffi(stream);
        let stream = PlayerId::from(stream);

        let Some(player) = players.get(stream) else {
            drop(players);
            error!("Player not found for stream {stream:?}");
            return;
        };

        player.can_receive_broadcasts.store(true, Ordering::Relaxed);
    }
}
