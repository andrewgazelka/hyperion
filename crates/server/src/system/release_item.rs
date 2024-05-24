use bvh_region::aabb::Aabb;
use evenio::prelude::*;
use glam::Vec3;
use tracing::instrument;
use valence_protocol::{packets::play, ByteAngle, VarInt, Velocity};
use valence_server::entity::EntityKind;

use crate::{
    components::{Arrow, EntityPhysics, FullEntityPose, Uuid},
    event::ReleaseItem,
    net::{Broadcast, Compose},
    system::sync_entity_position::PositionSyncMetadata,
};

#[derive(Query)]
pub struct ReleaseItemQuery<'a> {
    pose: &'a FullEntityPose,
}

#[instrument(skip_all, level = "trace")]
pub fn release_item(
    r: Receiver<ReleaseItem, ReleaseItemQuery>,
    broadcast: Single<&mut Broadcast>,
    compose: Compose,
    s: Sender<(
        Insert<Arrow>,
        Insert<EntityPhysics>,
        Insert<FullEntityPose>,
        Insert<PositionSyncMetadata>,
        Insert<Uuid>,
        Spawn,
    )>,
) {
    // TODO: Check that there is a bow and arrow
    tracing::info!("shoot arrow");

    let query = r.query;

    let id = s.spawn();

    #[expect(clippy::cast_possible_wrap, reason = "wrapping is ok in this case")]
    let entity_id = VarInt(id.index().0 as i32);

    let uuid = Uuid::from(uuid::Uuid::new_v4());

    // TODO: Get this value normally
    let initial_speed = 3.0;

    let (pitch_sin, pitch_cos) = query.pose.pitch.to_radians().sin_cos();
    let (yaw_sin, yaw_cos) = query.pose.yaw.to_radians().sin_cos();
    let velocity = Vec3::new(-pitch_cos * yaw_sin, -pitch_sin, pitch_cos * yaw_cos) * initial_speed;

    // TODO: Vanilla minecraft doesn't include x/z offsets
    let position = Vec3::new(
        yaw_sin.mul_add(-0.5, query.pose.position.x),
        query.pose.position.y + 1.5,
        yaw_cos.mul_add(0.5, query.pose.position.z),
    );

    s.insert(id, Arrow);
    s.insert(id, EntityPhysics {
        velocity,
        gravity: 0.05,
        drag: 0.01,
    });
    s.insert(id, FullEntityPose {
        position,
        yaw: query.pose.yaw,
        pitch: query.pose.pitch,
        bounding: Aabb::create(position, 0.5, 0.5), // TODO: use correct values
    });
    s.insert(id, PositionSyncMetadata::default());
    s.insert(id, uuid);

    let pkt = play::EntitySpawnS2c {
        entity_id,
        object_uuid: *uuid,
        kind: VarInt(EntityKind::ARROW.get()),
        position: position.as_dvec3(),
        pitch: ByteAngle::from_degrees(query.pose.pitch),
        yaw: ByteAngle::from_degrees(query.pose.yaw),
        head_yaw: ByteAngle::from_degrees(0.0),
        data: VarInt::default(),
        velocity: Velocity(velocity.to_array().map(|a| (a * 8000.0) as i16)),
    };

    broadcast.append(&pkt, &compose).unwrap();
}
