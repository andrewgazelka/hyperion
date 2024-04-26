use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    prelude::With,
    query::Query,
};
use tracing::instrument;

use crate::{
    components::{EntityReaction, FullEntityPose, ImmuneStatus, Player, Vitals},
    event::Gametick,
    global::Global,
    singleton::bounding_box::EntityBoundingBoxes,
};

#[derive(Query)]
pub struct PlayerDetectMobHitsQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
    reaction: &'a mut EntityReaction,
    vitals: &'a mut Vitals,
    immunity: &'a mut ImmuneStatus,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn player_detect_mob_hits(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&EntityBoundingBoxes>,
    global: Single<&Global>,
    mut poses_fetcher: Fetcher<PlayerDetectMobHitsQuery>,
) {
    poses_fetcher.iter_mut().for_each(|query| {
        let PlayerDetectMobHitsQuery {
            id,
            pose,
            reaction,
            vitals,
            immunity,
            _player,
        } = query;

        // todo: remove mid just use loc directly
        let this = pose.bounding.mid();

        entity_bounding_boxes
            .query
            .get_collisions(pose.bounding, |collision| {
                // do not include self
                if collision.id == id {
                    return true;
                }

                let other = collision.aabb.mid();

                let delta_x = other.x - this.x;
                let delta_z = other.z - this.z;

                if delta_x.abs() < 0.01 && delta_z.abs() < 0.01 {
                    // todo: implement like vanilla
                    return true;
                }

                let dist_xz = delta_x.hypot(delta_z);
                let multiplier = 0.4;

                reaction.velocity.x /= 2.0;
                reaction.velocity.y /= 2.0;
                reaction.velocity.z /= 2.0;
                reaction.velocity.x -= delta_x / dist_xz * multiplier;
                reaction.velocity.y += multiplier;
                reaction.velocity.z -= delta_z / dist_xz * multiplier;

                if reaction.velocity.y > 0.4 {
                    reaction.velocity.y = 0.4;
                }

                vitals.hurt(&global, 1.0, immunity);

                true
            });
    });
}
