use evenio::{
    entity::EntityId,
    event::{Insert, Receiver, Sender, Spawn},
    prelude::Single,
};
use rand_distr::{Distribution, LogNormal};
use tracing::instrument;
use valence_protocol::{ByteAngle, VarInt, Velocity};
use valence_server::entity::EntityKind;

use crate::{
    components::{
        Display, EntityReaction, FullEntityPose, ImmuneStatus, Npc, RunningSpeed, Uuid, Vitals,
    },
    event::InitEntity,
    net::{Broadcast, Compose},
    singleton::player_id_lookup::EntityIdLookup,
    system::sync_entity_position::PositionSyncMetadata,
};

#[tracing::instrument(skip_all)]
pub fn spawn_entity_packet(
    id: EntityId,
    kind: EntityKind,
    uuid: Uuid,
    pose: &FullEntityPose,
) -> valence_protocol::packets::play::EntitySpawnS2c {
    #[expect(clippy::cast_possible_wrap, reason = "wrapping is ok in this case")]
    let entity_id = VarInt(id.index().0 as i32);

    valence_protocol::packets::play::EntitySpawnS2c {
        entity_id,
        object_uuid: *uuid,
        kind: VarInt(kind.get()),
        position: pose.position.as_dvec3(),
        pitch: ByteAngle::from_degrees(pose.pitch),
        yaw: ByteAngle::from_degrees(pose.yaw),
        head_yaw: ByteAngle::from_degrees(pose.head_yaw()),
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
        Insert<Npc>,
        Insert<Uuid>,
        Insert<RunningSpeed>,
        Insert<EntityReaction>,
        Insert<Vitals>,
        Insert<ImmuneStatus>,
        Insert<Display>,
        Spawn,
    )>,
    broadcast: Single<&Broadcast>,
    compose: Compose,
) {
    let event = r.event;

    let id = s.spawn();

    let uuid = Uuid::from(uuid::Uuid::new_v4());

    s.insert(id, Npc);
    s.insert(id, event.pose);
    s.insert(id, uuid);
    s.insert(id, EntityReaction::default());
    s.insert(id, ImmuneStatus::default());
    s.insert(id, generate_running_speed());
    s.insert(id, PositionSyncMetadata::default());
    s.insert(id, Display(event.display));
    s.insert(id, Vitals::ALIVE);

    id_lookup.insert(id.index().0 as i32, id);

    let pose = event.pose;

    let pkt = spawn_entity_packet(id, EntityKind::ZOMBIE, uuid, &pose);

    // todo: use shared scratch if possible
    broadcast.append(&pkt, &compose).unwrap();
}

fn generate_running_speed() -> RunningSpeed {
    // Parameters for the Log-Normal distribution
    let mean = 0.10; // Mean of the underlying Normal distribution
    let std_dev = 0.20; // Standard deviation of the underlying Normal distribution
    let log_normal = LogNormal::new(mean, std_dev).unwrap();

    let speed = log_normal.sample(&mut rand::thread_rng()) * 0.1;
    RunningSpeed(speed)
}
