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
    components::{
        EntityReaction, FullEntityPose, ImmuneStatus, MinecraftEntity, RunningSpeed, Uuid, Vitals,
    },
    events::{InitEntity, Scratch},
    net::{Broadcast, Compressor, IoBufs},
    singleton::player_id_lookup::EntityIdLookup,
    system::entity_position::PositionSyncMetadata,
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
    mut id_lookup: Single<&mut EntityIdLookup>,
    mut s: Sender<(
        Insert<FullEntityPose>,
        Insert<PositionSyncMetadata>,
        Insert<MinecraftEntity>,
        Insert<Uuid>,
        Insert<RunningSpeed>,
        Insert<EntityReaction>,
        Insert<Vitals>,
        Insert<ImmuneStatus>,
        Spawn,
    )>,
    mut io: Single<&mut IoBufs>,
    broadcast: Single<&Broadcast>,
    mut compressor: Single<&mut Compressor>,
) {
    let event = r.event;

    let id = s.spawn();

    let uuid = Uuid::from(uuid::Uuid::new_v4());

    s.insert(id, MinecraftEntity);
    s.insert(id, event.pose);
    s.insert(id, uuid);
    s.insert(id, EntityReaction::default());
    s.insert(id, ImmuneStatus::default());
    s.insert(id, generate_running_speed());
    s.insert(id, PositionSyncMetadata::default());
    s.insert(id, Vitals::ALIVE);

    id_lookup.inner.insert(id.index().0 as i32, id);

    let pose = event.pose;

    let pkt = spawn_packet(id, uuid, &pose);

    // todo: use shared scratch if possible
    let mut scratch = Scratch::new();
    broadcast
        .append(&pkt, io.one(), &mut scratch, compressor.one())
        .unwrap();
}

fn generate_running_speed() -> RunningSpeed {
    // Parameters for the Log-Normal distribution
    let mean = 0.10; // Mean of the underlying Normal distribution
    let std_dev = 0.20; // Standard deviation of the underlying Normal distribution
    let log_normal = LogNormal::new(mean, std_dev).unwrap();

    let speed = log_normal.sample(&mut rand::thread_rng()) * 0.1;
    RunningSpeed(speed)
}
