use crate::{EntityReaction, FullEntityPose};

impl FullEntityPose {
    /// # Safety
    /// This is only safe is this is not done in tandem with another `EntityReaction`
    // #[instrument(skip_all)]
    pub unsafe fn apply_entity_collision(&self, other: &Self, reaction: &EntityReaction) {
        const MULT_FACTOR: f32 = 2.0;

        let dx = other.position.x - self.position.x;
        let dz = other.position.z - self.position.z;

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

            let reaction = &mut *reaction.0.get();

            reaction.velocity.x -= vx * MULT_FACTOR;
            reaction.velocity.z -= vz * MULT_FACTOR;

            // todo: more efficient to do this OR
            // more efficient to just have par iter
            // other.x_velocity += vx;
            // other.z_velocity += vz;
        }
    }
}
