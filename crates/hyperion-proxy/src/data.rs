use std::sync::{atomic::AtomicBool, Arc, RwLock};

use bytes::Bytes;
use slotmap::{new_key_type, KeyData};

use crate::cache::GlobalExclusionsManager;

new_key_type! {
    pub struct PlayerId;
}

impl From<u64> for PlayerId {
    fn from(id: u64) -> Self {
        let raw = KeyData::from_ffi(id);
        Self::from(raw)
    }
}

pub struct OrderedBytes {
    /// The player's order will need to be >= the order of the packet to send. Each packet sent
    /// will increase the order by 1.
    /// todo: handle wrapping around
    pub order: u32,
    pub data: Bytes,
    pub exclusions: Option<Arc<GlobalExclusionsManager>>,
}

impl OrderedBytes {
    pub const FLUSH: Self = Self {
        order: 0,
        data: Bytes::from_static(b"flush"),
        exclusions: None,
    };

    pub fn is_flush(&self) -> bool {
        self.data.as_ref() == b"flush" // todo: this is REALLY jank let's maybe not do this
    }

    pub const fn no_order(data: Bytes) -> Self {
        Self {
            order: 0,
            data,
            exclusions: None,
        }
    }

    pub const fn with_exclusions(
        order: u32,
        data: Bytes,
        exclusions: Arc<GlobalExclusionsManager>,
    ) -> Self {
        Self {
            order,
            data,
            exclusions: Some(exclusions),
        }
    }
}

impl PartialEq<Self> for OrderedBytes {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order
    }
}

impl Eq for OrderedBytes {}

impl PartialOrd<Self> for OrderedBytes {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrderedBytes {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.order.cmp(&other.order)
    }
}

#[derive(Debug)]
pub struct PlayerHandle {
    pub writer: tokio::sync::mpsc::Sender<OrderedBytes>,

    /// Whether the player is allowed to send broadcasts.
    ///
    /// This exists because the player is not automatically in the play state,
    /// and if they are not in the play state and they receive broadcasts,
    /// they will get packets that it deems are invalid because the broadcasts are using the play
    /// state and play IDs.
    pub can_receive_broadcasts: AtomicBool,
}

pub type PlayerRegistry = RwLock<slotmap::SlotMap<PlayerId, PlayerHandle>>;
