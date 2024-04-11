//! Lookup players by their UUID
use std::collections::HashMap;

use evenio::{entity::EntityId, prelude::Component};
use uuid::Uuid;

/// See [`crate::singleton::player_uuid_lookup`].
#[derive(Component, Default, Debug)]
pub struct PlayerUuidLookup {
    /// The UUID of all players
    inner: HashMap<Uuid, EntityId>,
}

impl PlayerUuidLookup {
    /// Insert a player into the lookup.
    pub fn insert(&mut self, uuid: Uuid, entity: EntityId) {
        self.inner.insert(uuid, entity);
    }

    /// Remove a player from the lookup.
    pub fn remove(&mut self, uuid: &Uuid) {
        self.inner.remove(uuid);
    }

    /// Get the entity id of a player.
    pub fn get(&self, uuid: &Uuid) -> Option<&EntityId> {
        self.inner.get(uuid)
    }
}
