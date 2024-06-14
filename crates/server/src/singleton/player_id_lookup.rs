//! Lookup players by their UUID

use dashmap::DashMap;
use derive_more::Deref;
use flecs_ecs::{core::Entity, macros::Component};

/// See [`crate::singleton::player_uuid_lookup`].
#[derive(Component, Default, Debug, Deref)]
pub struct EntityIdLookup {
    /// The UUID of all players
    inner: DashMap<i32, Entity>,
}
