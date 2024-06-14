//! Defines a singleton that is used to query given bounding boxes.
//! This uses a [`bvh_region::Bvh`] to accelerate collision detection and querying.
use bvh_region::{aabb::Aabb, HasAabb};
use flecs_ecs::{core::Entity, macros::Component};

/// An [`Aabb`] that is tied to an [`Entity`].
#[derive(Copy, Clone, Debug)]
pub struct Stored {
    /// The [`Aabb`] of the entity.
    pub aabb: Aabb,
    /// The [`Entity`] of the entity in the ECS framework.
    pub id: Entity,
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
    pub query: bvh_region::Bvh<Stored>,
}

impl EntityBoundingBoxes {
    /// Clears the bounding boxes.
    pub fn clear(&mut self) {
        self.query.clear();
    }
}
