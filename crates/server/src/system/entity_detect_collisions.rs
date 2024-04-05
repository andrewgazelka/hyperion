use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    rayon::prelude::*,
};
use tracing::instrument;

use crate::{
    bounding_box::{CollisionContext, EntityBoundingBoxes},
    EntityReaction, FullEntityPose, Gametick,
};

#[instrument(skip_all, name = "entity_detect_collisions")]
pub fn entity_detect_collisions(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&EntityBoundingBoxes>,
    poses_fetcher: Fetcher<(EntityId, &FullEntityPose, &EntityReaction)>,
) {
    let entity_bounding_boxes = entity_bounding_boxes.0;

    poses_fetcher.par_iter().for_each(|(id, pose, reaction)| {
        let context = CollisionContext {
            bounding: pose.bounding,
            id,
        };

        entity_bounding_boxes.get_collisions(&context, |collision| {
            if collision.id == id {
                return;
            }

            unsafe { pose.apply_entity_collision(&collision.aabb, reaction) };
        });
    });
}
