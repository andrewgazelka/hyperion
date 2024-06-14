//! Lookup players by their UUID

use dashmap::DashMap;
use derive_more::{Deref, DerefMut};
use flecs_ecs::{core::Entity, macros::Component};
pub type StreamId = u64;

/// See [`crate::singleton::player_uuid_lookup`].
#[derive(Component, Default, Debug, Deref, DerefMut)]
pub struct StreamLookup {
    /// The UUID of all players
    inner: DashMap<StreamId, Entity>,
}
