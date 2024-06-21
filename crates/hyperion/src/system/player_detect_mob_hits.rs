use std::cell::RefCell;

use evenio::{
    entity::EntityId,
    event::{Receiver, Sender},
    fetch::{Fetcher, Single},
    prelude::With,
    query::Query,
};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use rayon_local::RayonLocal;
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
    s: Sender<event::BulkShoved>,
) {
    let sender = RayonLocal::init(Vec::new).map(RefCell::new);

    poses_fetcher.par_iter_mut().for_each(|query| {
        let PlayerDetectMobHitsQuery { id, pose, _player } = query;

        let sender = sender.get_local();
        let mut sender = sender.borrow_mut();

        entity_bounding_boxes
            .query
            .get_collisions(pose.bounding, |collision| {
                // do not include self
                if collision.id == id {
                    return true;
                }

                sender.push(event::Shoved {
                    target: id,
                    from: collision.id,
                    from_location: collision.aabb.mid(),
                });

                true
            });
    });

    s.send(event::BulkShoved(sender));
}
