use evenio::{
    entity::EntityId,
    event::{Insert, Receiver, Sender, Spawn},
    prelude::Single,
};
use generator::EntityType;
use rand_distr::{Distribution, LogNormal};
use tracing::instrument;
use valence_protocol::{ByteAngle, VarInt, Velocity};

use crate::{
    singleton::broadcast::BroadcastBuf, EntityReaction, FullEntityPose, InitEntity,
    MinecraftEntity, RunningSpeed, Uuid,
};

pub fn spawn_packet(
    id: EntityId,
    uuid: Uuid,
    pose: &FullEntityPose,
) -> valence_protocol::packets::play::EntitySpawnS2c {
    #[expect(clippy::cast_possible_wrap, reason = "wrapping is ok in this case")]
    let entity_id = VarInt(id.index().0 as i32);

    valence_protocol::packets::play::EntitySpawnS2c {
        entity_id,
        object_uuid: uuid.0,
        kind: VarInt(EntityType::Zombie as i32),
        position: pose.position.as_dvec3(),
        pitch: ByteAngle::from_degrees(pose.pitch),
        yaw: ByteAngle::from_degrees(pose.yaw),
        head_yaw: ByteAngle(0),
        data: VarInt::default(),
        velocity: Velocity([0; 3]),
    }
}

#[instrument(skip_all)]
pub fn init_entity(
    r: Receiver<InitEntity>,
    mut s: Sender<(
        Insert<FullEntityPose>,
        Insert<MinecraftEntity>,
        Insert<Uuid>,
        Insert<RunningSpeed>,
        Insert<EntityReaction>,
        Spawn,
    )>,
    mut broadcast: Single<&mut BroadcastBuf>,
) {
    let event = r.event;

    let id = s.spawn();

    let uuid = Uuid(uuid::Uuid::new_v4());

    s.insert(id, MinecraftEntity);
    s.insert(id, event.pose);
    s.insert(id, uuid);
    s.insert(id, EntityReaction::default());
    s.insert(id, generate_running_speed());

    let pose = event.pose;

    let pkt = spawn_packet(id, uuid, &pose);

    broadcast.get_round_robin().append_packet(&pkt).unwrap();
}

fn generate_running_speed() -> RunningSpeed {
    // Parameters for the Log-Normal distribution
    let mean = 0.10; // Mean of the underlying Normal distribution
    let std_dev = 0.20; // Standard deviation of the underlying Normal distribution
    let log_normal = LogNormal::new(mean, std_dev).unwrap();

    let speed = log_normal.sample(&mut rand::thread_rng()) * 0.1;
    RunningSpeed(speed)
}
