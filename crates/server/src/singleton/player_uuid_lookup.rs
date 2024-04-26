//! Lookup players by their UUID
use std::collections::HashMap;

use derive_more::{Deref, DerefMut};
use evenio::{entity::EntityId, prelude::Component};
use uuid::Uuid;

/// See [`crate::singleton::player_uuid_lookup`].
#[derive(Component, Default, Debug, Deref, DerefMut)]
pub struct PlayerUuidLookup {
    /// The UUID of all players
    inner: HashMap<Uuid, EntityId>,
}
