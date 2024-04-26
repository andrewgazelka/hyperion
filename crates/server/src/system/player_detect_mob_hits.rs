use evenio::{
    entity::EntityId,
    event::{Receiver, Sender},
    fetch::{Fetcher, Single},
    prelude::With,
    query::Query,
};
use tracing::instrument;

use crate::{
    components::{FullEntityPose, Player},
    event,
    event::Gametick,
    singleton::bounding_box::EntityBoundingBoxes,
};

#[derive(Query)]
pub struct PlayerDetectMobHitsQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn player_detect_mob_hits(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&EntityBoundingBoxes>,
    mut poses_fetcher: Fetcher<PlayerDetectMobHitsQuery>,
    mut s: Sender<event::Shoved>,
) {
    poses_fetcher.iter_mut().for_each(|query| {
        let PlayerDetectMobHitsQuery { id, pose, _player } = query;

        entity_bounding_boxes
            .query
            .get_collisions(pose.bounding, |collision| {
                // do not include self
                if collision.id == id {
                    return true;
                }

                s.send(event::Shoved {
                    target: id,
                    from: collision.id,
                    from_location: collision.aabb.mid(),
                });

                true
            });
    });
}
