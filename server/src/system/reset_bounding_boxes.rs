use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    query::Query,
};
use tracing::instrument;

use crate::{bounding_box::EntityBoundingBoxes, FullEntityPose, Gametick};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    id: EntityId,
    pose: &'a mut FullEntityPose,
}

#[instrument(skip_all, name = "reset_bounding_boxes")]
pub fn call(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&mut EntityBoundingBoxes>,
    mut entities: Fetcher<EntityQuery>,
) {
    let entity_bounding_boxes = entity_bounding_boxes.0;

    entity_bounding_boxes.clear();

    // todo: make par iterator
    entities.iter_mut().for_each(|query| {
        let EntityQuery { id, pose } = query;

        let bounding = pose.bounding;

        entity_bounding_boxes.insert(bounding, id);
    });
}
