//! Lookup players by their UUID

use derive_more::{Deref, DerefMut};
use evenio::{entity::EntityId, prelude::Component};
use fxhash::FxHashMap;

pub type StreamId = u64;

/// See [`crate::singleton::player_uuid_lookup`].
#[derive(Component, Default, Debug, Deref, DerefMut)]
pub struct StreamLookup {
    /// The UUID of all players
    inner: FxHashMap<StreamId, EntityId>,
}
