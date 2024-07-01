//! Lookup players by their UUID

use derive_more::{Deref, DerefMut};
use flecs_ecs::{core::Entity, macros::Component};
use fxhash::FxHashMap;

/// See [`crate::singleton::player_uuid_lookup`].
#[derive(Component, Default, Debug, Deref, DerefMut)]
pub struct StreamLookup {
    /// The UUID of all players
    inner: FxHashMap<u64, Entity>,
}
