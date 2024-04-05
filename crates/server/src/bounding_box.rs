use std::iter::Zip;

use bvh::{aabb::Aabb, HasAabb};
use evenio::{component::Component, entity::EntityId};
use smallvec::SmallVec;
use valence_protocol::math::Vec3;

use crate::FullEntityPose;

#[derive(Copy, Clone, Debug)]
pub struct Stored {
    pub aabb: Aabb,
    pub id: EntityId,
}

impl HasAabb for Stored {
    fn aabb(&self) -> Aabb {
        self.aabb
    }
}

#[derive(Component, Default)]
pub struct EntityBoundingBoxes {
    pub query: bvh::Bvh<Stored>,
}

impl From<BoundingBox> for Aabb {
    fn from(value: BoundingBox) -> Self {
        Self::new([value.min.x, value.min.y, value.min.z], [
            value.max.x,
            value.max.y,
            value.max.z,
        ])
    }
}

#[derive(Copy, Clone, Debug)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl BoundingBox {
    #[must_use]
    pub fn move_to(&self, feet: Vec3) -> Self {
        let half_width = (self.max.x - self.min.x) / 2.0;
        let height = self.max.y - self.min.y;

        let min = Vec3::new(feet.x - half_width, feet.y, feet.z - half_width);
        let max = Vec3::new(feet.x + half_width, feet.y + height, feet.z + half_width);

        Self { min, max }
    }

    #[must_use]
    pub fn create(feet: Vec3, width: f32, height: f32) -> Self {
        let half_width = width / 2.0;

        let min = Vec3::new(feet.x - half_width, feet.y, feet.z - half_width);
        let max = Vec3::new(feet.x + half_width, feet.y + height, feet.z + half_width);

        Self { min, max }
    }

    #[must_use]
    pub fn move_by(&self, offset: Vec3) -> Self {
        Self {
            min: self.min + offset,
            max: self.max + offset,
        }
    }
}

pub struct Collisions {
    pub ids: SmallVec<EntityId, 4>,
    pub poses: SmallVec<FullEntityPose, 4>,
}

impl IntoIterator for Collisions {
    type IntoIter = Zip<smallvec::IntoIter<EntityId, 4>, smallvec::IntoIter<FullEntityPose, 4>>;
    type Item = (EntityId, FullEntityPose);

    fn into_iter(self) -> Self::IntoIter {
        self.ids.into_iter().zip(self.poses)
    }
}

pub struct CollisionContext {
    pub bounding: BoundingBox,
    pub id: EntityId,
}

impl EntityBoundingBoxes {
    // todo: is there a better way to do this
    pub fn clear(&mut self) {
        self.query.clear();
    }

    pub fn get_collisions(&self, current: &CollisionContext, process: impl FnMut(&Stored) -> bool) {
        let bounding = current.bounding.into();

        self.query.get_collisions(bounding, process);
    }
}

// https://www.youtube.com/watch?v=3s7h2MHQtxc
// order 1 hilbert is 2x2   (2^1 x 2^1)
// order 2 hilbert is 4x4   (2^2 x 2^2)
// order 3 hilbert is 8x8   (2^3 x 2^3)
// ...
// order 10 hilbert is 1024x1024   (2^10 x 2^10)

// 1024x1024
