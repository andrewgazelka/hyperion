//! Lookup players by their UUID
use std::collections::HashMap;

use evenio::{entity::EntityId, prelude::Component};

/// See [`crate::singleton::player_uuid_lookup`].
#[derive(Component, Default, Debug)]
pub struct PlayerIdLookup {
    /// The UUID of all players
    pub inner: HashMap<i32, EntityId>,
}
