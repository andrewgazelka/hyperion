use evenio::{
    entity::EntityId,
    event::{Insert, Receiver, Sender, Spawn},
    fetch::Fetcher,
    prelude::Single,
};
use generator::EntityType;
use rand_distr::{Distribution, LogNormal};
use tracing::{info, instrument};
use valence_protocol::{ByteAngle, VarInt, Velocity};

use crate::{
    components::{EntityReaction, FullEntityPose, MinecraftEntity, RunningSpeed, Uuid},
    events::InitEntity,
    global::Global,
    net::LocalEncoder,
    singleton::broadcast::BroadcastBuf,
    system::entity_position::PositionSyncMetadata,
};

pub fn spawn_packet(
    id: EntityId,
    uuid: Uuid,
    pose: &FullEntityPose,
) -> valence_protocol::packets::play::EntitySpawnS2c {
    #[expect(clippy::cast_possible_wrap, reason = "wrapping is ok in this case")]
    let entity_id = VarInt(id.index().0 as i32);

    info!("spawn packet for zombie with id {entity_id:?} pose {pose:?}");

    valence_protocol::packets::play::EntitySpawnS2c {
        entity_id,
        object_uuid: *uuid,
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
    // encoders: Fetcher<(&mut LocalEncoder)>,
    global: Single<&Global>,

    mut s: Sender<(
        Insert<FullEntityPose>,
        Insert<PositionSyncMetadata>,
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

    let uuid = Uuid::from(uuid::Uuid::new_v4());

    s.insert(id, MinecraftEntity);
    s.insert(id, event.pose);
    s.insert(id, uuid);
    s.insert(id, EntityReaction::default());
    s.insert(id, generate_running_speed());
    s.insert(id, PositionSyncMetadata::default());

    let pose = event.pose;

    let pkt = spawn_packet(id, uuid, &pose);

    // for encoder in encoders {
    //     encoder.append(&pkt, &global).unwrap();
    // }

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
