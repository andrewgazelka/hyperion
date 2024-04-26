use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
    query::{Query, With},
};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::instrument;
use valence_protocol::math::{Vec2, Vec3};

use crate::{
    components::{EntityReaction, FullEntityPose, MinecraftEntity, RunningSpeed},
    events::Gametick,
    singleton::player_aabb_lookup::PlayerBoundingBoxes,
};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    running_speed: Option<&'a RunningSpeed>,
    reaction: &'a mut EntityReaction,
    pose: &'a mut FullEntityPose,
    _entity: With<&'static MinecraftEntity>,
}

#[instrument(skip_all, level = "trace")]
pub fn entity_move_logic(
    _: Receiver<Gametick>,
    mut entities: Fetcher<EntityQuery>,
    lookup: Single<&PlayerBoundingBoxes>,
) {
    entities.par_iter_mut().for_each(|query| {
        let EntityQuery {
            running_speed,
            pose,
            reaction,
            ..
        } = query;

        let current = pose.position;

        let Some(target) = lookup.closest_to(current) else {
            return;
        };

        let dif_mid = target.aabb.mid() - current;
        // let dif_height = target.aabb.min.y - current.y;

        let dif2d = Vec2::new(dif_mid.x, dif_mid.z);

        let yaw = dif2d.y.atan2(dif2d.x).to_degrees();

        // subtract 90 degrees
        let yaw = yaw - 90.0;

        // let pitch = -dif_height.atan2(dif2d.length()).to_degrees();
        let pitch = 0.0;

        if dif2d.length_squared() < 0.01 {
            // info!("Moving entity {:?} by {:?}", id, reaction.velocity);
            pose.move_by(reaction.velocity);
        } else {
            // normalize
            let dif2d = dif2d.normalize();

            let speed = running_speed.copied().unwrap_or_default();
            let dif2d = dif2d * speed.0;

            let vec = Vec3::new(dif2d.x, 0.0, dif2d.y) + reaction.velocity;

            pose.move_by(vec);
        }

        pose.pitch = pitch;
        pose.yaw = yaw;

        reaction.velocity = Vec3::ZERO;
    });
}
