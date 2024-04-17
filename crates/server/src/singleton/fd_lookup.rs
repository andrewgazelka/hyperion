//! Lookup players by their UUID
use std::ops::{Deref, DerefMut};

use evenio::{entity::EntityId, prelude::Component};
use fxhash::FxHashMap;

use crate::net::Fd;

/// See [`crate::singleton::player_uuid_lookup`].
#[derive(Component, Default, Debug)]
pub struct FdLookup {
    /// The UUID of all players
    inner: FxHashMap<Fd, EntityId>,
}

impl DerefMut for FdLookup {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Deref for FdLookup {
    type Target = FxHashMap<Fd, EntityId>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
