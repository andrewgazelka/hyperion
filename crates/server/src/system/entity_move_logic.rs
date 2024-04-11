use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    query::{Query, With},
    rayon::prelude::*,
};
use tracing::instrument;
use valence_protocol::{
    math::{Vec2, Vec3},
    ByteAngle, VarInt,
};

use crate::{
    singleton::{
        broadcast::{BroadcastBuf, PacketMetadata, PacketNecessity},
        player_aabb_lookup::PlayerAabbs,
    },
    EntityReaction, FullEntityPose, Gametick, MinecraftEntity, RunningSpeed,
};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    id: EntityId,
    running_speed: Option<&'a RunningSpeed>,
    reaction: &'a mut EntityReaction,
    pose: &'a mut FullEntityPose,
    _entity: With<&'static MinecraftEntity>,
}

#[instrument(skip_all, level = "trace")]
pub fn entity_move_logic(
    _: Receiver<Gametick>,
    mut entities: Fetcher<EntityQuery>,
    broadcast: Single<&BroadcastBuf>,
    lookup: Single<&PlayerAabbs>,
) {
    use valence_protocol::packets::play;

    entities.par_iter_mut().for_each(|query| {
        let EntityQuery {
            id,
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

        reaction.velocity = Vec3::ZERO;

        #[expect(
            clippy::cast_possible_wrap,
            reason = "wrapping is okay in this scenario"
        )]
        let entity_id = VarInt(id.index().0 as i32);

        let pos = play::EntityPositionS2c {
            entity_id,
            position: pose.position.as_dvec3(),
            yaw: ByteAngle::from_degrees(yaw),
            pitch: ByteAngle::from_degrees(pitch),
            on_ground: false,
        };

        let look = play::EntitySetHeadYawS2c {
            entity_id,
            head_yaw: ByteAngle::from_degrees(yaw),
        };

        let metadata = PacketMetadata {
            necessity: PacketNecessity::Droppable {
                prioritize_location: Vec2::new(pose.position.x, pose.position.y),
            },
            exclude_player: None,
        };

        // todo: remove unwrap
        broadcast.append(&pos, metadata).unwrap();
        broadcast.append(&look, metadata).unwrap();
    });
}
