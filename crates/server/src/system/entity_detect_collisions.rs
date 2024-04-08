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

#[instrument(skip_all, level = "trace")]
pub fn entity_detect_collisions(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&EntityBoundingBoxes>,
    poses_fetcher: Fetcher<(EntityId, &FullEntityPose, &EntityReaction)>,
) {
    const MAX_COLLISIONS: usize = 4;

    poses_fetcher.par_iter().for_each(|(id, pose, reaction)| {
        let context = CollisionContext {
            bounding: pose.bounding,
            id,
        };

        let mut collisions = 0;
        entity_bounding_boxes.get_collisions(&context, |collision| {
            // do not include self
            if collision.id == id {
                return true;
            }

            collisions += 1;

            // short circuit if we have too many collisions
            if collisions >= MAX_COLLISIONS {
                return false;
            }

            unsafe { pose.apply_entity_collision(&collision.aabb, reaction) };

            true
        });
    });
}
