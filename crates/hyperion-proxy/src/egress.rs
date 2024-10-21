use std::sync::{atomic::Ordering, Arc};

use arc_swap::ArcSwap;
use bvh::{Aabb, Bvh};
use glam::I16Vec2;
use hyperion_proto::{ChunkPosition, ServerToProxyMessage};
use rustc_hash::FxBuildHasher;
use tracing::{debug, error, instrument, warn};

use crate::{
    cache::GlobalExclusionsManager,
    data::{OrderedBytes, PlayerHandle},
};

#[derive(Default, Debug)]
struct PositionData {
    streams: Vec<u64>,
    positions: Vec<ChunkPosition>,
}

pub struct Egress {
    player_registry: &'static papaya::HashMap<u64, PlayerHandle, FxBuildHasher>,
    positions: ArcSwap<PositionData>,
}

pub struct BroadcastLocalInstruction {
    pub order: u32,
    pub bvh: Arc<Bvh<bytes::Bytes>>,
}

impl Egress {
    #[must_use]
    pub fn new(registry: &'static papaya::HashMap<u64, PlayerHandle, FxBuildHasher>) -> Self {
        Self {
            player_registry: registry,
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
        // todo: why cannot I pin_owned inside the spawn
        let players = self.player_registry.pin_owned();

        tokio::task::Builder::new()
            .name("broadcast_global")
            .spawn(async move {
                let data = pkt.data;

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
            })
            .unwrap();
    }

    #[instrument(skip_all)]
    pub fn handle_flush(&self) {
        let players = self.player_registry.pin_owned();

        tokio::spawn(async move {
            for player in players.values() {
                if let Err(e) = player.writer.try_send(OrderedBytes::FLUSH) {
                    warn!("Failed to send data to player: {:?}", e);
                }
            }
        });
    }

    pub fn handle_broadcast_local(self: Arc<Self>, instruction: BroadcastLocalInstruction) {
        let order = instruction.order;
        let bvh = instruction.bvh;

        // we are spawning because it is rather intensive to call get_in_slices on a bvh
        // #[allow(clippy::significant_drop_tightening)]
        tokio::task::Builder::new()
            .name("broadcast_local")
            .spawn(async move {
                const RADIUS: i16 = 8;

                let positions = self.positions.load();
                let players = self.player_registry.pin();

                for (id, &position) in positions.streams.iter().zip(positions.positions.iter()) {
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
            })
            .unwrap();
    }

    #[instrument(skip(self, pkt))]
    pub fn handle_multicast(&self, pkt: hyperion_proto::Multicast) {
        let players = self.player_registry.pin_owned();
        let data = pkt.data;

        tokio::spawn(async move {
            let players = pkt.stream.iter().filter_map(|id| players.get(id));
            for player in players {
                let to_send = OrderedBytes::no_order(data.clone());
                // todo: handle error; kick player if cannot send (buffer full)
                if let Err(e) = player.writer.try_send(to_send) {
                    debug!("Failed to send data to player: {:?}", e);
                }
            }
        });
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

        let players = self.player_registry.pin();

        let Some(player) = players.get(&id) else {
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
        let player_registry = self.player_registry;
        let players = player_registry.pin();
        let stream = pkt.stream;

        let Some(player) = players.get(&stream) else {
            error!("Player not found for stream {stream:?}");
            return;
        };

        player.can_receive_broadcasts.store(true, Ordering::Relaxed);
    }
}
