use std::{ops::Range, sync::Arc};

use bytes::BytesMut;
use hyperion_proto::{BroadcastGlobal, ServerToProxyMessage, UpdatePlayerChunkPositions};
use slotmap::{KeyData, SecondaryMap};

use crate::{data::PlayerId, egress::Egress};

#[derive(Default, Copy, Clone)]
pub struct ExcludeNode {
    pub prev: u32,
    pub exclusion_start: u32,
    pub exclusion_end: u32,
}

impl ExcludeNode {
    const PLACEHOLDER: Self = Self {
        prev: 0,
        exclusion_start: 0,
        exclusion_end: 0,
    };
}

pub struct GlobalExclusions {
    pub nodes: Vec<ExcludeNode>,
    pub player_to_end_node: SecondaryMap<PlayerId, u32>,
}

impl Default for GlobalExclusions {
    fn default() -> Self {
        Self {
            nodes: vec![ExcludeNode::PLACEHOLDER],
            player_to_end_node: SecondaryMap::new(),
        }
    }
}

impl GlobalExclusions {
    #[must_use]
    pub fn take(&mut self) -> Self {
        // todo: probably a better way to do this
        let result = Self {
            nodes: self.nodes.clone(),
            player_to_end_node: self.player_to_end_node.clone(),
        };

        *self = Self::default();

        result
    }

    pub fn exclusions_for_player(
        &self,
        player_id: PlayerId,
    ) -> impl Iterator<Item = Range<usize>> + '_ {
        ExclusionIterator::new(self, player_id)
    }

    pub fn append(&mut self, player_id: PlayerId, range: Range<usize>) {
        let exclusion_start = u32::try_from(range.start).expect("Exclusion start is too large");
        let exclusion_end = u32::try_from(range.end).expect("Exclusion end is too large");

        if let Some(current_end) = self.player_to_end_node.get(player_id) {
            let idx = self.nodes.len();
            self.nodes.push(ExcludeNode {
                prev: *current_end,
                exclusion_start,
                exclusion_end,
            });
            self.player_to_end_node.insert(player_id, idx as u32);
            return;
        }

        let idx = self.nodes.len();
        self.nodes.push(ExcludeNode {
            prev: 0,
            exclusion_start,
            exclusion_end,
        });
        self.player_to_end_node.insert(player_id, idx as u32);
    }
}

struct ExclusionIterator<'a> {
    exclusions: &'a GlobalExclusions,
    current: u32,
}

impl<'a> ExclusionIterator<'a> {
    fn new(exclusions: &'a GlobalExclusions, player_id: PlayerId) -> Self {
        let end = exclusions
            .player_to_end_node
            .get(player_id)
            .copied()
            .unwrap_or_default();

        Self {
            exclusions,
            current: end,
        }
    }
}

impl<'a> Iterator for ExclusionIterator<'a> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == 0 {
            return None;
        }

        let node = self.exclusions.nodes.get(self.current as usize)?;
        let start = usize::try_from(node.exclusion_start).expect("Exclusion start is too large");
        let end = usize::try_from(node.exclusion_end).expect("Exclusion end is too large");

        self.current = node.prev;

        Some(start..end)
    }
}

pub struct BufferedEgress {
    broadcast_required: BytesMut,

    exclusions: GlobalExclusions,

    update_chunk_positions: Option<UpdatePlayerChunkPositions>,

    egress: Arc<Egress>,

    broadcast_order: Option<u32>,
}

impl BufferedEgress {
    pub fn new(egress: Arc<Egress>) -> Self {
        Self {
            broadcast_required: BytesMut::new(),
            update_chunk_positions: None,
            exclusions: GlobalExclusions::default(),
            egress,
            broadcast_order: None,
        }
    }

    // #[instrument(skip_all)]
    pub fn handle_packet(&mut self, message: ServerToProxyMessage) {
        match message {
            ServerToProxyMessage::UpdatePlayerChunkPositions(packet) => {
                self.update_chunk_positions = Some(packet);
            }
            ServerToProxyMessage::BroadcastGlobal(packet) => {
                if let Some(order) = self.broadcast_order
                    && order != packet.order
                {
                    // todo: remove packet
                    let pkt = BroadcastGlobal {
                        data: self.broadcast_required.split().freeze(),
                        optional: false,
                        exclude: 0,
                        order,
                    };

                    self.egress
                        .handle_broadcast_global(pkt, self.exclusions.take());
                }

                self.broadcast_order = Some(packet.order);

                // // todo: care about optional
                let current_len = self.broadcast_required.len();
                self.broadcast_required.extend_from_slice(&packet.data);

                if packet.exclude != 0 {
                    let new_len = self.broadcast_required.len();
                    // todo: there might be a case where entity id is 0 but I am pretty sure it is
                    // NonZero
                    let key = KeyData::from_ffi(packet.exclude);
                    let key = PlayerId::from(key);
                    self.exclusions.append(key, current_len..new_len);
                }

                // todo: If we can, let's try to flush before the flush packet is called, if it
                // is useful. Let's make it so if we get beyond L1 and L2
                // cache limits, we automatically send the bytes buffer.
                // I think this makes sense to do; perhaps it doesn't, but I'm pretty sure it
                // does. Because if we have a giant amount of bytes that we
                // want to send, we'll have to be getting data from the L3
                // cache. If it's over a certain size, then that's not good,
                // and the cache would be invalid for every single player probably.

                // self.egress
                //     .handle_broadcast_global(packet, self.exclusions.take());
                //
                // if let Some(update_chunk_positions) = self.update_chunk_positions.take() {
                //     self.egress
                //         .handle_packet(ServerToProxyMessage::UpdatePlayerChunkPositions(
                //             update_chunk_positions,
                //         ));
                // }
            }
            // todo: impl
            ServerToProxyMessage::BroadcastLocal(_) | ServerToProxyMessage::Multicast(_) => {}
            pkt @ ServerToProxyMessage::Unicast(_) => self.egress.handle_packet(pkt),
            pkt @ ServerToProxyMessage::SetReceiveBroadcasts(..) => {
                self.egress.handle_packet(pkt);
            }
            ServerToProxyMessage::Flush(_) => {
                if let Some(order) = self.broadcast_order {
                    let pkt = BroadcastGlobal {
                        data: self.broadcast_required.split().freeze(),
                        optional: false,
                        exclude: 0,
                        order,
                    };

                    self.egress
                        .handle_broadcast_global(pkt, self.exclusions.take());
                }

                self.broadcast_order = None;

                self.egress.handle_flush();

                // let pkt = BroadcastGlobal {
                //     data: self.broadcast_required.split().freeze(),
                //     optional: false,
                //     exclude: 0,
                //     order : 0,
                // };

                // todo!("order");

                // todo: Properly handle exclude.
                // self.egress
                //     .handle_broadcast_global(pkt, self.exclusions.take());
                //
                // if let Some(update_chunk_positions) = self.update_chunk_positions.take() {
                //     self.egress
                //         .handle_packet(ServerToProxyMessage::UpdatePlayerChunkPositions(
                //             update_chunk_positions,
                //         ));
                // }
            }
        }
    }
}
