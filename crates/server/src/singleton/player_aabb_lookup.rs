//! A singleton designed for querying players based on their bounding boxes.
use bvh::{aabb::Aabb, HasAabb};
use evenio::{entity::EntityId, prelude::Component};

/// The data associated with a player
#[derive(Debug, Copy, Clone)]
pub struct LookupData {
    /// The entity id of the player
    #[expect(dead_code, reason = "this is not being used yet")]
    pub id: EntityId,
    /// The bounding box of the player
    pub aabb: Aabb,
}

impl HasAabb for LookupData {
    fn aabb(&self) -> Aabb {
        self.aabb
    }
}

/// See [`crate::singleton::player_aabb_lookup`].
#[derive(Component, Debug, Default)]
pub struct PlayerAabbs {
    /// The bounding boxes of all players
    pub inner: bvh::Bvh<LookupData>,
}

impl PlayerAabbs {
    /// Get the closest player to the given position.
    pub fn closest_to(&self, point: glam::Vec3) -> Option<&LookupData> {
        let (target, _) = self.inner.get_closest(point)?;
        Some(target)
    }
}
