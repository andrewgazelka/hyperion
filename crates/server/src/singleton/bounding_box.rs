use bvh::{aabb::Aabb, HasAabb};
use evenio::{component::Component, entity::EntityId};

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

impl EntityBoundingBoxes {
    pub fn clear(&mut self) {
        self.query.clear();
    }

    pub fn get_collisions(&self, bounding: Aabb, process: impl FnMut(&Stored) -> bool) {
        self.query.get_collisions(bounding, process);
    }
}
