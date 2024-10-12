use std::{ops::Range, sync::Arc};

use bytes::BytesMut;
use hyperion_proto::{BroadcastGlobal, ServerToProxyMessage, UpdatePlayerChunkPositions};
use slotmap::{KeyData, SecondaryMap};

use crate::{data::PlayerId, egress::Egress};

/// Represents a node in the exclusion list, containing information about the previous node
/// and the range of the exclusion.
#[derive(Default, Copy, Clone)]
pub struct ExclusionNode {
    /// Index of the previous node in the exclusion list.
    pub prev_index: u32,
    /// Start of the exclusion range.
    pub range_start: u32,
    /// End of the exclusion range.
    pub range_end: u32,
}

impl ExclusionNode {
    /// A placeholder node used as the initial element in the exclusion list.
    const PLACEHOLDER: Self = Self {
        prev_index: 0,
        range_start: 0,
        range_end: 0,
    };
}

/// Manages global exclusions for players.
pub struct ExclusionManager {
    /// List of exclusion nodes.
    pub nodes: Vec<ExclusionNode>,
    /// Maps player IDs to their last exclusion node index.
    pub player_to_last_node: SecondaryMap<PlayerId, u32>,
}

impl Default for ExclusionManager {
    fn default() -> Self {
        Self {
            nodes: vec![ExclusionNode::PLACEHOLDER],
            player_to_last_node: SecondaryMap::new(),
        }
    }
}

impl ExclusionManager {
    /// Takes ownership of the current exclusion data and resets the manager.
    #[must_use]
    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }

    /// Returns an iterator over the exclusions for a specific player.
    pub fn exclusions_for_player(
        &self,
        player_id: PlayerId,
    ) -> impl Iterator<Item = Range<usize>> + '_ {
        ExclusionIterator::new(self, player_id)
    }

    /// Appends a new exclusion range for a player.
    pub fn append(&mut self, player_id: PlayerId, range: Range<usize>) {
        let range_start = u32::try_from(range.start).expect("Exclusion start is too large");
        let range_end = u32::try_from(range.end).expect("Exclusion end is too large");

        let new_node = ExclusionNode {
            prev_index: self
                .player_to_last_node
                .get(player_id)
                .copied()
                .unwrap_or(0),
            range_start,
            range_end,
        };

        let new_index = self.nodes.len() as u32;
        self.nodes.push(new_node);
        self.player_to_last_node.insert(player_id, new_index);
    }
}

/// Iterator for traversing exclusions for a specific player.
struct ExclusionIterator<'a> {
    exclusions: &'a ExclusionManager,
    current_index: u32,
}

impl<'a> ExclusionIterator<'a> {
    fn new(exclusions: &'a ExclusionManager, player_id: PlayerId) -> Self {
        let start_index = exclusions
            .player_to_last_node
            .get(player_id)
            .copied()
            .unwrap_or_default();

        Self {
            exclusions,
            current_index: start_index,
        }
    }
}

impl Iterator for ExclusionIterator<'_> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index == 0 {
            return None;
        }

        let node = self.exclusions.nodes.get(self.current_index as usize)?;
        let start = usize::try_from(node.range_start).expect("Exclusion start is too large");
        let end = usize::try_from(node.range_end).expect("Exclusion end is too large");

        self.current_index = node.prev_index;

        Some(start..end)
    }
}

/// Buffers egress operations for optimized processing.
pub struct BufferedEgress {
    /// Buffer for required broadcast data.
    broadcast_buffer: BytesMut,
    /// Manages player-specific exclusions.
    exclusion_manager: ExclusionManager,
    /// Stores pending chunk position updates.
    pending_chunk_update: Option<UpdatePlayerChunkPositions>,
    /// Reference to the underlying egress handler.
    egress: Arc<Egress>,
    /// Tracks the current broadcast order.
    current_broadcast_order: Option<u32>,
}

impl BufferedEgress {
    /// Creates a new `BufferedEgress` instance.
    pub fn new(egress: Arc<Egress>) -> Self {
        Self {
            broadcast_buffer: BytesMut::new(),
            exclusion_manager: ExclusionManager::default(),
            pending_chunk_update: None,
            egress,
            current_broadcast_order: None,
        }
    }

    /// Handles incoming server-to-proxy messages.
    // #[instrument(skip_all)]
    pub fn handle_packet(&mut self, message: ServerToProxyMessage) {
        match message {
            ServerToProxyMessage::UpdatePlayerChunkPositions(packet) => {
                self.pending_chunk_update = Some(packet);
            }
            ServerToProxyMessage::BroadcastGlobal(packet) => {
                if let Some(order) = self.current_broadcast_order
                    && order != packet.order
                {
                    self.flush_broadcast(order);
                }

                self.current_broadcast_order = Some(packet.order);

                let current_len = self.broadcast_buffer.len();
                self.broadcast_buffer.extend_from_slice(&packet.data);

                if packet.exclude != 0 {
                    let new_len = self.broadcast_buffer.len();
                    let key = KeyData::from_ffi(packet.exclude);
                    let player_id = PlayerId::from(key);
                    self.exclusion_manager
                        .append(player_id, current_len..new_len);
                }

                // TODO: Consider implementing auto-flush based on buffer size
                // to optimize cache usage.
            }
            ServerToProxyMessage::BroadcastLocal(_) | ServerToProxyMessage::Multicast(_) => {
                // TODO: Implement handling for these message types
            }
            pkt @ ServerToProxyMessage::Unicast(_) => self.egress.handle_packet(pkt),
            pkt @ ServerToProxyMessage::SetReceiveBroadcasts(..) => {
                self.egress.handle_packet(pkt);
            }
            ServerToProxyMessage::Flush(_) => {
                if let Some(order) = self.current_broadcast_order.take() {
                    self.flush_broadcast(order);
                }

                self.egress.handle_flush();
            }
        }
    }

    /// Flushes the current broadcast buffer.
    fn flush_broadcast(&mut self, order: u32) {
        let pkt = BroadcastGlobal {
            data: self.broadcast_buffer.split().freeze(),
            optional: false,
            exclude: 0,
            order,
        };

        self.egress
            .handle_broadcast_global(pkt, self.exclusion_manager.take());
    }
}
