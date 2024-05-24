use evenio::prelude::*;
use glam::Vec3;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::instrument;

use crate::{
    components::{EntityPhysics, FullEntityPose},
    Gametick,
};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    physics: &'a mut EntityPhysics,
    pose: &'a mut FullEntityPose,
}

#[instrument(skip_all, level = "trace")]
pub fn entity_physics(_: Receiver<Gametick>, mut entities: Fetcher<EntityQuery>) {
    entities.par_iter_mut().for_each(|query| {
        let EntityQuery { physics, pose } = query;

        pose.position += physics.velocity;

        // TODO: Living entities and explosive projectiles have drag applied after gravity

        // Drag is applied before gravity
        physics.velocity = ((physics.velocity - Vec3::new(0.0, physics.gravity, 0.0))
            * (1.0 - physics.drag))
            - Vec3::new(0.0, physics.gravity, 0.0);
    });
}
