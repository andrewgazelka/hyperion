use std::{ops::Range, sync::Arc};

use bvh::{Bvh, Data, Point};
use glam::I16Vec2;
use hyperion_proto::{ArchivedServerToProxyMessage, BroadcastGlobal};
use rustc_hash::FxBuildHasher;

use crate::egress::{BroadcastLocalInstruction, Egress};

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
pub struct GlobalExclusionsManager {
    /// List of exclusion nodes.
    pub nodes: Vec<ExclusionNode>,
    /// Maps player IDs to their last exclusion node index.
    pub player_to_last_node: std::collections::HashMap<u64, u32, FxBuildHasher>,
}

impl Default for GlobalExclusionsManager {
    fn default() -> Self {
        Self {
            nodes: vec![ExclusionNode::PLACEHOLDER],
            player_to_last_node: std::collections::HashMap::default(),
        }
    }
}

impl GlobalExclusionsManager {
    /// Takes ownership of the current exclusion data and resets the manager.
    #[must_use]
    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }

    /// Returns an iterator over the exclusions for a specific player.
    pub fn exclusions_for_player(&self, player_id: u64) -> impl Iterator<Item = Range<usize>> + '_ {
        ExclusionIterator::new(self, player_id)
    }

    /// Appends a new exclusion range for a player.
    pub fn append(&mut self, player_id: u64, range: Range<usize>) {
        let range_start = u32::try_from(range.start).expect("Exclusion start is too large");
        let range_end = u32::try_from(range.end).expect("Exclusion end is too large");

        let new_node = ExclusionNode {
            prev_index: self
                .player_to_last_node
                .get(&player_id)
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
    exclusions: &'a GlobalExclusionsManager,
    current_index: u32,
}

impl<'a> ExclusionIterator<'a> {
    fn new(exclusions: &'a GlobalExclusionsManager, player_id: u64) -> Self {
        let start_index = exclusions
            .player_to_last_node
            .get(&player_id)
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LocalBroadcastData {
    position: I16Vec2,
    range_start: usize,
    range_end: usize,
}

impl Point for LocalBroadcastData {
    fn point(&self) -> I16Vec2 {
        self.position
    }
}

impl Data for LocalBroadcastData {
    type Context<'a> = &'a Vec<u8>;
    type Unit = u8;

    fn data<'a: 'c, 'b: 'c, 'c>(&'a self, context: &'b Vec<u8>) -> &'c [Self::Unit] {
        unsafe { context.get_unchecked(self.range_start..self.range_end) }
    }
}

/// Buffers egress operations for optimized processing.
pub struct BufferedEgress {
    /// Buffer for required broadcast data.
    global_broadcast_buffer: Vec<u8>,

    raw_local_broadcast_data: Vec<u8>,
    local_broadcast_buffer: Vec<LocalBroadcastData>,

    /// Manages player-specific exclusions.
    exclusion_manager: GlobalExclusionsManager,
    /// Reference to the underlying egress handler.
    egress: Egress,
    /// Tracks the current broadcast order.
    current_broadcast_order: Option<u32>,
    local_flush_counter: u32,
}

impl BufferedEgress {
    /// Creates a new `BufferedEgress` instance.
    #[must_use]
    pub fn new(egress: Egress) -> Self {
        Self {
            global_broadcast_buffer: Vec::new(),
            raw_local_broadcast_data: vec![],
            local_broadcast_buffer: Vec::default(),
            exclusion_manager: GlobalExclusionsManager::default(),
            egress,
            current_broadcast_order: None,
            local_flush_counter: 0,
        }
    }

    /// Handles incoming server-to-proxy messages.
    // #[instrument(skip_all)]
    pub fn handle_packet(&mut self, message: &ArchivedServerToProxyMessage<'_>) {
        match message {
            ArchivedServerToProxyMessage::UpdatePlayerChunkPositions(packet) => {
                self.egress.handle_update_player_chunk_positions(packet);
            }
            ArchivedServerToProxyMessage::BroadcastGlobal(packet) => {
                let Ok(packet_order) = rkyv::deserialize::<u32, !>(&packet.order);

                if let Some(order) = self.current_broadcast_order
                    && order != packet_order
                {
                    // send the current broadcasts to all players
                    self.flush_broadcast(order);
                }

                self.current_broadcast_order = Some(packet_order);

                let current_len = self.global_broadcast_buffer.len();
                self.global_broadcast_buffer.extend_from_slice(&packet.data);

                let Ok(packet_exclude) = rkyv::deserialize::<u64, !>(&packet.exclude);

                if packet_exclude != 0 {
                    // we need to exclude a player
                    let new_len = self.global_broadcast_buffer.len();
                    self.exclusion_manager
                        .append(packet_exclude, current_len..new_len);
                }

                // TODO: Consider implementing auto-flush based on buffer size
                // to optimize cache usage.
            }
            ArchivedServerToProxyMessage::BroadcastLocal(packet) => {
                let Ok(center_x) = rkyv::deserialize::<i16, !>(&packet.center.x);
                let Ok(center_z) = rkyv::deserialize::<i16, !>(&packet.center.z);

                let position = I16Vec2::new(center_x, center_z);

                let before_len = self.raw_local_broadcast_data.len();
                self.raw_local_broadcast_data
                    .extend_from_slice(&packet.data);
                let after_len = self.raw_local_broadcast_data.len();

                self.local_broadcast_buffer.push(LocalBroadcastData {
                    // todo: checked
                    position,
                    range_start: before_len,
                    range_end: after_len,
                });
            }
            ArchivedServerToProxyMessage::Unicast(unicast) => {
                self.egress.handle_unicast(unicast);
            }
            ArchivedServerToProxyMessage::SetReceiveBroadcasts(pkt) => {
                self.egress.handle_set_receive_broadcasts(pkt);
            }
            ArchivedServerToProxyMessage::Flush(_) => {
                if let Some(order) = self.current_broadcast_order.take() {
                    self.flush_broadcast(order);
                }

                self.egress.handle_flush();
                self.local_flush_counter = 0;

                let local_broadcast_buffer = core::mem::take(&mut self.local_broadcast_buffer);

                if local_broadcast_buffer.is_empty() {
                    return;
                }

                let raw_local_broadcast_data = core::mem::take(&mut self.raw_local_broadcast_data);

                let egress = self.egress;
                tokio::task::Builder::new()
                    .name("bvh")
                    .spawn(async move {
                        let bvh = Bvh::build(local_broadcast_buffer, &raw_local_broadcast_data);

                        let bvh = bvh.into_bytes();

                        let instruction = BroadcastLocalInstruction {
                            order: 0,
                            bvh: Arc::new(bvh),
                        };

                        egress.handle_broadcast_local(instruction);
                    })
                    .unwrap();
            }
        }
    }

    /// Flushes the current broadcast buffer.
    fn flush_broadcast(&mut self, order: u32) {
        let data = &self.global_broadcast_buffer;
        let pkt = BroadcastGlobal {
            data,
            exclude: 0,
            order,
        };

        let exclusions = self.exclusion_manager.take();

        self.egress.handle_broadcast_global(pkt, exclusions);
        self.global_broadcast_buffer.clear();
    }
}
