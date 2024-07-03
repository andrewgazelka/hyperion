use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use arc_swap::ArcSwap;
use bvh::{Aabb, Bvh, Data, Point};
use hyperion_proto::ServerToProxyMessage;
use slotmap::KeyData;
use tracing::{debug, error, info, instrument, warn};

use crate::{
    cache::GlobalExclusions,
    data::{OrderedBytes, PlayerId, PlayerRegistry},
};

#[derive(Default)]
pub struct Egress {
    bvh: ArcSwap<Bvh<u64>>,
    registry: Arc<PlayerRegistry>,
}

impl Egress {
    #[must_use]
    #[instrument]
    pub fn new(registry: Arc<PlayerRegistry>) -> Self {
        Self {
            bvh: ArcSwap::default(),
            registry,
        }
    }

    pub fn handle_packet(self: &Arc<Self>, packet: ServerToProxyMessage) {
        match packet {
            ServerToProxyMessage::UpdatePlayerChunkPositions(pkt) => {
                self.handle_update_player_chunk_positions(&pkt);
            }
            ServerToProxyMessage::BroadcastGlobal(_pkt) => {
                todo!();
                // self.handle_broadcast_global(pkt);
            }
            ServerToProxyMessage::BroadcastLocal(pkt) => {
                self.clone().handle_broadcast_local(pkt);
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
            ServerToProxyMessage::Flush(_) => {}
        }
    }

    #[instrument(skip(self, pkt))]
    pub fn handle_update_player_chunk_positions(
        &self,
        pkt: &hyperion_proto::UpdatePlayerChunkPositions,
    ) {
        // todo: handle case where we are getting updates too fast
        let positions: Vec<_> = (0..pkt.positions.len())
            .map(|i| PlayerChunkPosRef {
                parent: pkt,
                idx: i,
            })
            .collect();

        let len = positions.len();
        let bvh = Bvh::build(positions, len);

        self.bvh.store(Arc::new(bvh));
        info!("Updated player chunk positions and stored new BVH");
    }

    #[instrument(skip(self, pkt, exclusions), level = "trace")]
    pub fn handle_broadcast_global(
        &self,
        pkt: hyperion_proto::BroadcastGlobal,
        exclusions: GlobalExclusions,
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

            let to_send = OrderedBytes::with_exclusions(data.clone(), exclusions.clone());

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

    #[instrument(skip(self, pkt))]
    pub fn handle_broadcast_local(self: Arc<Self>, pkt: hyperion_proto::BroadcastLocal) {
        let center = pkt.center.expect("center is required");
        let radius = pkt.taxicab_radius as i16;

        let center = glam::I16Vec2::new(center.x as i16, center.z as i16);
        let min = center - glam::I16Vec2::splat(radius);
        let max = center + glam::I16Vec2::splat(radius);

        let aabb = Aabb::new(min, max);
        let data = pkt.data;

        // we are spawning because it is rather intensive to call get_in_slices on a bvh
        #[allow(clippy::significant_drop_tightening)]
        tokio::spawn(async move {
            // todo: ArrayVec might overflow
            let bvh = self.bvh.load();
            let player_ids = bvh.get_in_slices(aabb).into_iter().flatten();

            let players = self.registry.read().unwrap();
            for id in player_ids {
                let id = KeyData::from_ffi(*id);
                let id = PlayerId::from(id);

                let Some(player) = players.get(id) else {
                    // expected to still happen infrequently
                    debug!("Player not found for id {id:?}");
                    continue;
                };

                if !player.can_receive_broadcasts.load(Ordering::Relaxed) {
                    continue;
                }

                let to_send = OrderedBytes::no_order(data.clone());

                // todo: handle error; kick player if cannot send (buffer full)
                if let Err(e) = player.writer.try_send(to_send) {
                    debug!("Failed to send data to player: {:?}", e);
                }
            }
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
        // delay a second

        let registry = self.registry.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
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
        });

        // let players = self.registry.read().unwrap();
        // let stream = pkt.stream;
        // let stream = KeyData::from_ffi(stream);
        // let stream = PlayerId::from(stream);
        //
        // let Some(player) = players.get(stream) else {
        //     drop(players);
        //     error!("Player not found for stream {stream:?}");
        //     return;
        // };
        //
        // player.can_receive_broadcasts.store(true, Ordering::Relaxed);
    }
}

struct PlayerChunkPosRef<'a> {
    parent: &'a hyperion_proto::UpdatePlayerChunkPositions,
    idx: usize,
}

impl<'a> Point for PlayerChunkPosRef<'a> {
    fn point(&self) -> glam::I16Vec2 {
        let position = &self.parent.positions[self.idx].clone();
        glam::I16Vec2::new(position.x as i16, position.z as i16)
    }
}

impl<'a> Data for PlayerChunkPosRef<'a> {
    type Unit = u64;

    fn data(&self) -> &[Self::Unit] {
        let elem = &self.parent.stream[self.idx];
        core::slice::from_ref(elem)
    }
}
