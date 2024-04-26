use bvh::TrivialHeuristic;
use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    query::Query,
};
use tracing::{instrument, span};

use crate::{
    components::FullEntityPose,
    events::Gametick,
    singleton::bounding_box::{EntityBoundingBoxes, Stored},
};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
}

#[instrument(skip_all, level = "trace")]
pub fn reset_bounding_boxes(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&mut EntityBoundingBoxes>,
    entities: Fetcher<EntityQuery>,
) {
    let entity_bounding_boxes = entity_bounding_boxes.0;

    entity_bounding_boxes.clear();

    // todo: make par iterator
    let stored: Vec<_> = span!(tracing::Level::TRACE, "entities-to-vec").in_scope(|| {
        entities
            .iter()
            .map(|query| Stored {
                aabb: query.pose.bounding,
                id: query.id,
            })
            .collect()
    });

    let bvh = bvh::Bvh::build::<TrivialHeuristic>(stored);

    entity_bounding_boxes.query = bvh;
}
