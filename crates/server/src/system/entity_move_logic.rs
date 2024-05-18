use evenio::{
    event::Receiver,
    fetch::Fetcher,
    query::{Query, With},
};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::instrument;
use valence_protocol::math::{Vec2, Vec3};

use crate::{
    components::{EntityReaction, FullEntityPose, Npc, RunningSpeed},
    event::Gametick,
};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    running_speed: Option<&'a RunningSpeed>,
    reaction: &'a mut EntityReaction,
    pose: &'a mut FullEntityPose,
    _entity: With<&'static Npc>,
}

#[derive(Copy, Clone)]
struct MakeSync<T>(T);

#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "todo: remove. https://discord.com/channels/273534239310479360/1120124565591425034/1241512520553074798"
)]
unsafe impl<T> Send for MakeSync<T> {}
unsafe impl<T> Sync for MakeSync<T> {}

#[allow(clippy::redundant_locals, reason = "lookup = lookup is not redundant; it initiates a copy -> move")]
#[instrument(skip_all, level = "trace")]
pub fn entity_move_logic(gametick: Receiver<Gametick>, mut entities: Fetcher<EntityQuery>) {
    let lookup = &gametick.event.player_bounding_boxes;

    let lookup = MakeSync(lookup);
    
    entities.par_iter_mut().for_each(|query| {
        let EntityQuery {
            running_speed,
            pose,
            reaction,
            ..
        } = query;

        let current = pose.position;
        
        let lookup = lookup;

        let Some((target, _)) = lookup.0.get_closest(current) else {
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
