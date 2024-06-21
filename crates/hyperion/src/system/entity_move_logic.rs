use flecs_ecs::core::{ReactorAPI, World};
use valence_protocol::math::{Vec2, Vec3};

use crate::{
    components::{EntityReaction, FullEntityPose, Npc, RunningSpeed},
    singleton::player_aabb_lookup::PlayerBoundingBoxes,
};

pub fn register_system(world: &World) {
    world
        .system::<(
            &mut EntityReaction,
            &mut FullEntityPose,
            &RunningSpeed,
            &Npc,
        )>()
        .each(|(mut reaction, mut pose, mut running_speed, _npc)| {
            let current = pose.position;

            let target = world.map::<&PlayerBoundingBoxes, _>(|x| x.closest_to(current));

            let Some(target) = target else {
                return;
            };

            let dif_mid = target.aabb.mid() - current;

            let dif2d = Vec2::new(dif_mid.x, dif_mid.z);

            let yaw = dif2d.y.atan2(dif2d.x).to_degrees();

            // subtract 90 degrees
            let yaw = yaw - 90.0;
            // // let pitch = -dif_height.atan2(dif2d.length()).to_degrees();
            let pitch = 0.0;
            //
            if dif2d.length_squared() < 0.01 {
                // info!("Moving entity {:?} by {:?}", id, reaction.velocity);
                pose.move_by(reaction.velocity);
            } else {
                // normalize
                let dif2d = dif2d.normalize();

                let speed = *running_speed;
                let dif2d = dif2d * speed.0;

                let vec = Vec3::new(dif2d.x, 0.0, dif2d.y) + reaction.velocity;

                pose.move_by(vec);
            }

            pose.pitch = pitch;
            pose.yaw = yaw;

            reaction.velocity = Vec3::ZERO;
        });
}
