use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    rayon::prelude::*,
};
use tracing::instrument;

use crate::{bounding_box::EntityBoundingBoxes, EntityReaction, FullEntityPose, Gametick};

#[instrument(skip_all, level = "trace")]
pub fn entity_detect_collisions(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&EntityBoundingBoxes>,
    mut poses_fetcher: Fetcher<(EntityId, &FullEntityPose, &mut EntityReaction)>,
) {
    const MAX_COLLISIONS: usize = 4;

    poses_fetcher
        .par_iter_mut()
        .for_each(|(id, pose, reaction)| {
            let mut collisions = 0;
            entity_bounding_boxes.get_collisions(pose.bounding, |collision| {
                // do not include self
                if collision.id == id {
                    return true;
                }

                collisions += 1;

                // short circuit if we have too many collisions
                if collisions >= MAX_COLLISIONS {
                    return false;
                }

                pose.apply_entity_collision(&collision.aabb, reaction);

                true
            });
        });
}
