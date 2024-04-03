use bvh::{aabb::Aabb, HasAabb};
use evenio::{entity::EntityId, prelude::Component};

#[derive(Debug, Copy, Clone)]
pub struct LookupData {
    pub id: EntityId,
    pub aabb: Aabb,
}

impl HasAabb for LookupData {
    fn aabb(&self) -> Aabb {
        self.aabb
    }
}

#[derive(Component, Debug, Default)]
pub struct PlayerLocationLookup {
    pub inner: bvh::Bvh<LookupData>,
}

impl PlayerLocationLookup {
    pub fn closest_to(&self, point: glam::Vec3) -> Option<&LookupData> {
        let (target, _) = self.inner.get_closest(point)?;
        Some(target)
    }
}
