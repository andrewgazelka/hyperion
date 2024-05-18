use bvh_region::{aabb::Aabb, HasAabb, TrivialHeuristic};
use evenio::{
    entity::EntityId,
    event::ReceiverMut,
    fetch::Fetcher,
    query::{Query, With},
};
use tracing::instrument;

use crate::{
    components::{FullEntityPose, Player},
    event::Gametick,
};

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

#[derive(Query, Debug)]
pub(crate) struct EntityQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn rebuild_player_location(
    mut gametick: ReceiverMut<Gametick>,
    entities: Fetcher<EntityQuery>,
) {
    let gametick = &mut *gametick.event;
    let bump = gametick.allocator();

    let mut elements = Vec::with_capacity_in(entities.iter().len(), bump);

    for query in &entities {
        elements.push(LookupData {
            id: query.id,
            aabb: query.pose.bounding,
        });
    }

    let bvh = bvh_region::Bvh::build_in::<TrivialHeuristic>(elements, bump);
    gametick.player_bounding_boxes = bvh;
}
