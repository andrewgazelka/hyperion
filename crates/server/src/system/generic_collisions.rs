use evenio::{
    entity::EntityId,
    event::{Receiver, Sender},
    fetch::{Fetcher, Single},
    query::Query,
};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use rayon_local::RayonLocal;
use tracing::{instrument, warn};

use crate::{
    components::FullEntityPose,
    event::{self, Gametick, GenericBulkCollitionEvent},
    singleton::bounding_box::EntityBoundingBoxes,
};

#[derive(Query)]
pub struct GenericCollisionQuery<'a, Q: Query> {
    id: EntityId,
    pose: &'a FullEntityPose,
    custom: Q,
}

#[instrument(skip_all, level = "trace")]
pub fn generic_collision<'a, Q>(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&EntityBoundingBoxes>,
    mut poses_fetcher: Fetcher<GenericCollisionQuery<'a, Q>>,
    s: Sender<GenericBulkCollitionEvent>,
) where
    Q: for<'__a> Query<Item<'__a> = Q> + Send + Clone,
{
    let sender = RayonLocal::init(Vec::new);

    poses_fetcher.par_iter_mut().for_each(|query| {
        let sender = sender.get_local_raw();
        let sender = unsafe { &mut *sender.get() };

        let GenericCollisionQuery { id, pose, custom } = query;

        entity_bounding_boxes
            .query
            .get_collisions(pose.bounding, |collision| {
                // do not include self
                if collision.id == id {
                    return true;
                }
                sender.push(event::Collision {
                    enitiy_id: id,
                    other_entity_id: collision.id,
                });

                true
            });
    });

    s.send(event::GenericBulkCollitionEvent { events: sender });
}
