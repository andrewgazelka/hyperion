use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    query::With,
};
use rayon::prelude::*;
use tracing::{info, instrument};

use crate::{
    components::{EntityReaction, FullEntityPose, Npc},
    event::Gametick,
    singleton::bounding_box::EntityBoundingBoxes,
};

#[instrument(skip_all, level = "trace")]
pub fn entity_detect_collisions(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&EntityBoundingBoxes>,
    mut poses_fetcher: Fetcher<(EntityId, &FullEntityPose, &mut EntityReaction, With<&Npc>)>,
) {
    const MAX_COLLISIONS: usize = 4;

    poses_fetcher
        .par_iter_mut()
        .for_each(|(id, pose, reaction, _)| {
            let mut collisions = 0;
            entity_bounding_boxes
                .query
                .get_collisions(pose.bounding, |collision| {
                    // do not include self
                    if collision.id == id {
                        return true;
                    }

                    collisions += 1;

                    // short circuit if we have too many collisions
                    if collisions >= MAX_COLLISIONS {
                        return false;
                    }

                    // println!("colliding with {id:?}");
                    info!("colliding with {id:?}");
                    pose.apply_entity_collision(&collision.aabb, reaction);

                    true
                });
        });
}
