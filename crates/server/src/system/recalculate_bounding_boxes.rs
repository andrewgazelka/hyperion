use bvh_region::TrivialHeuristic;
use evenio::{entity::EntityId, event::ReceiverMut, fetch::Fetcher, query::Query};
use tracing::{instrument, span};

use crate::{components::FullEntityPose, event::Gametick, system::LookupData};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
}

#[instrument(skip_all, level = "trace")]
pub fn recalculate_bounding_boxes(
    mut gametick: ReceiverMut<Gametick>,
    entities: Fetcher<EntityQuery>,
) {
    let gametick = &mut *gametick.event;

    // todo: make par iterator
    let stored = span!(tracing::Level::TRACE, "entities-to-vec").in_scope(|| {
        let mut res = Vec::with_capacity_in(entities.iter().len(), gametick.allocator());

        for query in &entities {
            res.push(LookupData {
                aabb: query.pose.bounding,
                id: query.id,
            });
        }

        res
    });

    let bvh = bvh_region::Bvh::build_in::<TrivialHeuristic>(stored, gametick.allocator());

    gametick.entity_bounding_boxes = bvh;
}
