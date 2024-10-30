use std::sync::{atomic::Ordering, Arc};

use bvh::{Aabb, Bvh};
use bytes::Bytes;
use glam::I16Vec2;
use hyperion_proto::{
    ArchivedSetReceiveBroadcasts, ArchivedUnicast, ArchivedUpdatePlayerChunkPositions,
    ChunkPosition,
};
use rustc_hash::FxBuildHasher;
use tracing::{debug, error, info_span, instrument, warn, Instrument};

use crate::{
    cache::ExclusionsManager,
    data::{OrderedBytes, PlayerHandle},
};

#[derive(Copy, Clone)]
pub struct Egress {
    // todo: can we do some type of EntityId and SlotMap
    player_registry: &'static papaya::HashMap<u64, PlayerHandle, FxBuildHasher>,

    // todo: remove positions when player leaves
    positions: &'static papaya::HashMap<u64, ChunkPosition, FxBuildHasher>,
}

pub struct BroadcastLocalInstruction {
    pub order: u32,
    pub bvh: Arc<Bvh<Bytes>>,
    pub exclusions: Arc<ExclusionsManager>,
}

impl Egress {
    #[must_use]
    pub const fn new(
        player_registry: &'static papaya::HashMap<u64, PlayerHandle, FxBuildHasher>,
        positions: &'static papaya::HashMap<u64, ChunkPosition, FxBuildHasher>,
    ) -> Self {
        Self {
            player_registry,
            positions,
        }
    }

    #[instrument(skip_all)]
    pub fn handle_update_player_chunk_positions(
        &mut self,
        pkt: &ArchivedUpdatePlayerChunkPositions,
    ) {
        let positions = self.positions.pin();
        for (stream, position) in pkt.stream.iter().zip(pkt.positions.iter()) {
            let Ok(stream) = rkyv::deserialize::<u64, !>(stream);

            // todo: can I just grab the whole thing as Infallible?
            let Ok(position_x) = rkyv::deserialize::<_, !>(&position.x);
            let Ok(position_z) = rkyv::deserialize::<_, !>(&position.z);

            let position = ChunkPosition {
                x: position_x,
                z: position_z,
            };

            positions.insert(stream, position);
        }
    }

    #[instrument(skip_all)]
    pub fn handle_broadcast_global(
        &self,
        pkt: hyperion_proto::BroadcastGlobal<'_>,
        exclusions: ExclusionsManager,
    ) {
        // todo: why cannot I pin_owned inside the spawn
        let players = self.player_registry.pin_owned();
        let data = pkt.data;
        let data = Bytes::copy_from_slice(data);

        tokio::task::Builder::new()
            .name("broadcast_global")
            .spawn(
                async move {
                    let exclusions = Arc::new(exclusions);

                    // imo it makes sense to read once... it is a fast loop
                    #[allow(clippy::significant_drop_in_scrutinee)]
                    for (player_id, player) in &players {
                        if !player.can_receive_broadcasts.load(Ordering::Relaxed) {
                            continue;
                        }

                        let to_send = OrderedBytes::with_exclusions(
                            pkt.order,
                            data.clone(),
                            exclusions.clone(),
                        );

                        match player.writer.try_send(to_send) {
                            Ok(true) => {} // success
                            Ok(false) => {
                                // player is disconnect
                                if let Some(result) = players.remove(player_id) {
                                    result.shutdown();
                                }

                                warn!("Failed to send data to player due to channel being full");
                            }
                            Err(e) => {
                                warn!("Failed to send data to player: {:?}", e);
                                if let Some(result) = players.remove(player_id) {
                                    result.shutdown();
                                }
                            }
                        }
                    }
                }
                .instrument(info_span!("broadcast_global_task")),
            )
            .unwrap();
    }

    #[instrument(skip_all)]
    pub fn handle_flush(&self) {
        let players = self.player_registry.pin_owned();

        tokio::spawn(
            async move {
                for (id, player) in &players {
                    match player.writer.try_send(OrderedBytes::FLUSH) {
                        Ok(true) => {} // success
                        Ok(false) => {
                            // player is disconnect
                            if let Some(result) = players.remove(id) {
                                result.shutdown();
                            }
                            warn!("Failed to send data to player due to channel being full");
                        }
                        Err(e) => {
                            warn!("Failed to send data to player: {:?}", e);
                            if let Some(result) = players.remove(id) {
                                result.shutdown();
                            }
                        }
                    }
                }
            }
            .instrument(info_span!("flush_task")),
        );
    }

    #[instrument(skip_all)]
    pub fn handle_broadcast_local(self, instruction: BroadcastLocalInstruction) {
        let order = instruction.order;
        let bvh = instruction.bvh;
        let exclusions = instruction.exclusions;

        let positions = self.positions.pin_owned();
        // we are spawning because it is rather intensive to call get_in_slices on a bvh
        // #[allow(clippy::significant_drop_tightening)]
        tokio::task::Builder::new()
            .name("broadcast_local")
            .spawn(
                async move {
                    const RADIUS: i16 = 16;

                    let players = self.player_registry.pin();

                    for (id, &position) in &positions {
                        let Some(player) = players.get(id) else {
                            // expected to still happen infrequently
                            debug!("Player not found for id {id:?}");
                            continue;
                        };

                        if !player.can_receive_broadcasts.load(Ordering::Relaxed) {
                            continue;
                        }

                        let position = I16Vec2::new(position.x, position.z);
                        let min = position - I16Vec2::splat(RADIUS);
                        let max = position + I16Vec2::splat(RADIUS);

                        let aabb = Aabb::new(min, max);

                        let slices = bvh.get_in(aabb);

                        for slice in slices {
                            let (_, data) = bvh.inner();

                            let start = slice.start as usize;
                            let end = slice.end as usize;

                            let data = data.slice(start..end);

                            let to_send = OrderedBytes {
                                order,
                                offset: slice.start,
                                data,
                                exclusions: Some(exclusions.clone()),
                            };

                            match player.writer.try_send(to_send) {
                                Ok(true) => {} // success
                                Ok(false) => {
                                    // player is disconnect
                                    if let Some(result) = players.remove(id) {
                                        result.shutdown();
                                    }
                                    warn!(
                                        "Failed to send data to player due to channel being full"
                                    );
                                }
                                Err(e) => {
                                    warn!("Failed to send data to player: {:?}", e);
                                    if let Some(result) = players.remove(id) {
                                        result.shutdown();
                                    }
                                }
                            }
                        }
                    }
                }
                .instrument(info_span!("broadcast_local_task")),
            )
            .unwrap();
    }

    #[instrument(skip_all)]
    pub fn handle_unicast(&self, pkt: &ArchivedUnicast<'_>) {
        let data = &pkt.data;
        let data = data.to_vec();
        let data = bytes::Bytes::from(data);

        let Ok(order) = rkyv::deserialize::<u32, !>(&pkt.order);

        let ordered = OrderedBytes {
            order,
            data,
            ..OrderedBytes::DEFAULT
        };

        let Ok(id) = rkyv::deserialize::<u64, !>(&pkt.stream);

        let players = self.player_registry.pin();

        let Some(player) = players.get(&id) else {
            // expected to still happen infrequently
            debug!("Player not found for id {id:?}");
            return;
        };

        // todo: handle error; kick player if cannot send (buffer full)
        match player.writer.try_send(ordered) {
            Ok(true) => {} // success
            Ok(false) => {
                // player is disconnect
                if let Some(result) = players.remove(&id) {
                    result.shutdown();
                }
                warn!("Failed to send data to player due to channel being full");
            }
            Err(e) => {
                warn!("Failed to send data to player: {:?}", e);
                if let Some(result) = players.remove(&id) {
                    result.shutdown();
                }
            }
        }

        drop(players);
    }

    #[instrument(skip_all)]
    pub fn handle_set_receive_broadcasts(&self, pkt: &ArchivedSetReceiveBroadcasts) {
        let player_registry = self.player_registry;
        let players = player_registry.pin();
        let Ok(stream) = rkyv::deserialize::<u64, !>(&pkt.stream);

        let Some(player) = players.get(&stream) else {
            error!("Player not found for stream {stream:?}");
            return;
        };

        player.can_receive_broadcasts.store(true, Ordering::Relaxed);
    }
}
