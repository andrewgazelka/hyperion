use bvh_region::aabb::Aabb;

use crate::components::{EntityReaction, FullEntityPose};

impl FullEntityPose {
    /// # Safety
    /// This is only safe is this is not done in tandem with another `EntityReaction`
    // #[instrument(skip_all)]
    pub fn apply_entity_collision(&self, other: &Aabb, reaction: &mut EntityReaction) {
        /// the multiplication factor comparatively to vanilla.
        /// This is useful for <https://github.com/andrewgazelka/hyperion/pull/111#issuecomment-2039030028>
        const MULT_FACTOR: f32 = 1.0;

        let dx = other.mid().x - self.position.x;
        let dz = other.mid().z - self.position.z;

        let largest_distance = dx.abs().max(dz.abs());

        if largest_distance >= 0.01 {
            let mut vx = dx / 20.0;
            let mut vz = dz / 20.0;

            if largest_distance < 1.0 {
                // 1 / sqrt(x) increases exponentially as x approaches 0

                vx /= largest_distance.sqrt();
                vz /= largest_distance.sqrt();
            } else {
                vx /= largest_distance;
                vz /= largest_distance;
            }

            reaction.velocity.x -= vx * MULT_FACTOR;
            reaction.velocity.z -= vz * MULT_FACTOR;

            // todo: more efficient to do this OR
            // more efficient to just have par iter
            // other.x_velocity += vx;
            // other.z_velocity += vz;
        }
    }
}
