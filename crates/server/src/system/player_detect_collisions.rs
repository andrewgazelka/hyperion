use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    query::With,
    rayon::prelude::*,
};
use tracing::instrument;

use crate::{
    singleton::bounding_box::EntityBoundingBoxes, EntityReaction, FullEntityPose, Gametick, Player,
};

#[instrument(skip_all, level = "trace")]
pub fn player_detect_collisions(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&EntityBoundingBoxes>,
    mut poses_fetcher: Fetcher<(
        EntityId,
        &FullEntityPose,
        &mut EntityReaction,
        With<&Player>,
    )>,
) {
    poses_fetcher
        .par_iter_mut()
        .for_each(|(id, pose, reaction, _)| {
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

                    // Ensure the combined square of delta_x and delta_z is not too small
                    // loop {
                    //     if delta_x * delta_x + delta_z * delta_z >= 0.0001 {
                    //         return true;
                    //     }
                    //     delta_x = (fastrand::f32() - 0.5) * 0.01;
                    //     delta_z = (fastrand::f32() - 0.5) * 0.01;
                    // }
                    //         this.getEntityAttribute(SharedMonsterAttributes.attackDamage).setBaseValue(3.0D);
                    //         float f = (float)this.getEntityAttribute(SharedMonsterAttributes.attackDamage).getAttributeValue();

                    // impl of knockback
                    // let _amount = 3.0;

                    // this.isAirBorne = true;
                    // float f = MathHelper.sqrt_double(p_70653_3_ * p_70653_3_ + p_70653_5_ * p_70653_5_);
                    let f = delta_x.hypot(delta_z);
                    //             float f1 = 0.4F;
                    let f1 = 0.4;

                    // this.motionX /= 2.0D;
                    // this.motionY /= 2.0D;
                    // this.motionZ /= 2.0D;
                    // this.motionX -= p_70653_3_ / (double)f * (double)f1;
                    // this.motionY += (double)f1;
                    // this.motionZ -= p_70653_5_ / (double)f * (double)f1;
                    reaction.velocity.x /= 2.0;
                    reaction.velocity.y /= 2.0;
                    reaction.velocity.z /= 2.0;
                    reaction.velocity.x -= delta_x / f * f1;
                    reaction.velocity.y += f1;
                    reaction.velocity.z -= delta_z / f * f1;

                    //             if (this.motionY > 0.4000000059604645D)
                    //             {
                    //                 this.motionY = 0.4000000059604645D;
                    //             }
                    if reaction.velocity.y > 0.4 {
                        reaction.velocity.y = 0.4;
                    }

                    true
                });
        });
}
