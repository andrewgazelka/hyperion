//! Defines a singleton that is used to query given bounding boxes.
//! This uses a [`bvh::Bvh`] to accelerate collision detection and querying.
use bvh::{aabb::Aabb, HasAabb};
use evenio::{component::Component, entity::EntityId};

/// An [`Aabb`] that is tied to an [`EntityId`].
#[derive(Copy, Clone, Debug)]
pub struct Stored {
    /// The [`Aabb`] of the entity.
    pub aabb: Aabb,
    /// The [`EntityId`] of the entity in the ECS framework.
    pub id: EntityId,
}

impl HasAabb for Stored {
    fn aabb(&self) -> Aabb {
        self.aabb
    }
}

/// See [`crate::singleton::bounding_box`].
#[derive(Component, Default)]
pub struct EntityBoundingBoxes {
    /// The bounding boxes of all entities as stored in a BVH.
    pub query: bvh::Bvh<Stored>,
}

impl EntityBoundingBoxes {
    /// Clears the bounding boxes.
    pub fn clear(&mut self) {
        self.query.clear();
    }
}
