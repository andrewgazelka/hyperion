//! Lookup players by their UUID

use derive_more::{Deref, DerefMut};
use evenio::{entity::EntityId, prelude::Component};
use fxhash::FxHashMap;

/// See [`crate::singleton::player_uuid_lookup`].
#[derive(Component, Default, Debug, Deref, DerefMut)]
pub struct EntityIdLookup {
    /// The UUID of all players
    inner: FxHashMap<i32, EntityId>,
}
