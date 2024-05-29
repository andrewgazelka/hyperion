use evenio::prelude::*;
use glam::{DVec3, IVec3, Vec3};
use line_drawing::{VoxelOrigin, WalkVoxels};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::instrument;
use valence_protocol::BlockPos;

use crate::{
    components::{chunks::Chunks, EntityPhysics, EntityPhysicsState, FullEntityPose},
    Gametick,
};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    physics: &'a mut EntityPhysics,
    pose: &'a mut FullEntityPose,
}

#[instrument(skip_all, level = "trace")]
pub fn entity_physics(
    _: Receiver<Gametick>,
    mut entities: Fetcher<EntityQuery>,
    chunks: Single<&Chunks>,
) {
    entities.par_iter_mut().for_each(|query| {
        let EntityQuery { physics, pose } = query;

        // TODO: Handle the situation where the block that the entity is stuck on is broken

        let EntityPhysicsState::Moving { velocity } = &mut physics.state else {
            return;
        };

        // `try_normalize` will only fail if the velocity is zero, in which case adjusting the
        // position is not needed.
        if let Some(direction) = velocity.try_normalize() {
            let direction_dvec = DVec3::from(direction);
            let old_position = pose.position;
            let old_position_dvec = DVec3::from(old_position);
            let new_position = old_position + *velocity;

            // Check for collisions. Entities may be moving faster than one block per tick, so all
            // blocks between the old position and new position need to be checked. This iterates
            // through every block between the old position and the new position and assumes that
            // the movement between the old position and new position is linear, which is not true, but
            // works good enough to simulate physics.
            for block_position in WalkVoxels::<f32, i32>::new(
                old_position.into(),
                new_position.into(),
                &VoxelOrigin::Corner,
            ) {
                let block_position_dvec = DVec3::from(IVec3::from(block_position));
                let block_position = BlockPos::from(block_position);
                // TODO: Handle entities going into unloaded chunks to avoid unwrap
                let block = chunks.get_block(block_position).unwrap();

                // TODO: Consider rewriting ray intersection algorithm to be able to use f32
                // instead of f64. However, collision shapes are provided using f64s, so there
                // would also be a cost to converting f64 to f32.
                let distance_travelled_until_collision = block
                    .collision_shapes()
                    .map(|aabb| {
                        (aabb + block_position_dvec)
                            .ray_intersection(old_position_dvec, direction_dvec)
                    })
                    .filter_map(|x| x) // Only keep collisions, not misses.
                    .map(|collision| collision[0]) // Keep the near position.
                    .fold(f64::INFINITY, |a, b| a.min(b)); // Get the lowest value in this iterator which will get the earliest collision that will occur.

                if distance_travelled_until_collision != f64::INFINITY {
                    // A collision occured.
                    pose.position =
                        old_position + direction * (distance_travelled_until_collision as f32);
                    physics.state = EntityPhysicsState::Stuck { block_position };
                    return;
                }
            }

            pose.position = new_position;
        }

        // TODO: Living entities and explosive projectiles have drag applied after gravity

        // Drag is applied before gravity. This formula is from
        // "Drag applied before acceleration" in https://minecraft.fandom.com/wiki/Entity,
        // modified to assume ticksPassed is 1. Acceleration refers to gravity in that page.
        *velocity = ((*velocity - Vec3::new(0.0, physics.gravity, 0.0)) * (1.0 - physics.drag))
            - Vec3::new(0.0, physics.gravity, 0.0);
    });
}
