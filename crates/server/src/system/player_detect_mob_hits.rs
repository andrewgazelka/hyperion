use evenio::{
    entity::EntityId,
    event::{Receiver, Sender},
    fetch::Fetcher,
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
};

#[derive(Query)]
pub struct PlayerDetectMobHitsQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn player_detect_mob_hits(
    gametick: Receiver<Gametick>,
    mut poses_fetcher: Fetcher<PlayerDetectMobHitsQuery>,
    mut s: Sender<event::BulkShoved>,
) {
    let sender = RayonLocal::init(Vec::new);

    let event = gametick.event;
    let bounding_boxes = &event.entity_bounding_boxes;

    poses_fetcher.par_iter_mut().for_each(|query| {
        let PlayerDetectMobHitsQuery { id, pose, _player } = query;

        let sender = sender.get_local_raw();
        let sender = unsafe { &mut *sender.get() };

        bounding_boxes.get_collisions(pose.bounding, |collision| {
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
