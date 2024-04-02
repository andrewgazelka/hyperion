use std::collections::HashMap;

use evenio::{entity::EntityId, prelude::Component};
use uuid::Uuid;

#[derive(Component, Default, Debug)]
pub struct PlayerLookup {
    inner: HashMap<Uuid, EntityId>,
}

impl PlayerLookup {
    pub fn insert(&mut self, uuid: Uuid, entity: EntityId) {
        self.inner.insert(uuid, entity);
    }

    pub fn remove(&mut self, uuid: &Uuid) {
        self.inner.remove(uuid);
    }

    pub fn get(&self, uuid: &Uuid) -> Option<&EntityId> {
        self.inner.get(uuid)
    }
}
