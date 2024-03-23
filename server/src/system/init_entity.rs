use evenio::{
    event::{Insert, Receiver, Sender, Spawn},
    fetch::Fetcher,
};
use generator::EntityType;
use rand_distr::{Distribution, LogNormal};
use valence_protocol::{ByteAngle, VarInt, Velocity};

use crate::{FullEntityPose, InitEntity, MinecraftEntity, Player, RunningSpeed, Uuid};

// Packet ID	State	Bound To	Field Name	Field Type	Notes
// 0x01	Play	Client	Entity ID	VarInt	A unique integer ID mostly used in the protocol to identify the
// entity. Entity UUID	UUID	A unique identifier that is mostly used in persistence and places where
// the uniqueness matters more. Type	VarInt	The type of the entity (see "type" field of the list of
// Mob types). X	Double
// Y	Double
// Z	Double
// Pitch	Angle	To get the real pitch, you must divide this by (256.0F / 360.0F)
// Yaw	Angle	To get the real yaw, you must divide this by (256.0F / 360.0F)
// Head Yaw	Angle	Only used by living entities, where the head of the entity may differ from the
// general body rotation. Data	VarInt	Meaning dependent on the value of the Type field, see Object
// Data for details. Velocity X	Short	Same units as Set Entity Velocity.

pub fn call(
    r: Receiver<InitEntity>,
    mut players: Fetcher<&mut Player>,
    mut s: Sender<(
        Insert<FullEntityPose>,
        Insert<MinecraftEntity>,
        Insert<Uuid>,
        Insert<RunningSpeed>,
        Spawn,
    )>,
) {
    use valence_protocol::packets::play;

    // random uuid using rand

    let event = r.event;

    let id = s.spawn();

    let uuid = Uuid(uuid::Uuid::new_v4());

    s.insert(id, MinecraftEntity);
    s.insert(id, event.pose);
    s.insert(id, uuid);

    // speed normal dist centered at 0.1
    // Parameters for the Log-Normal distribution
    let mean = 0.10; // Mean of the underlying Normal distribution
    let std_dev = 0.20; // Standard deviation of the underlying Normal distribution
    let log_normal = LogNormal::new(mean, std_dev).unwrap();

    let speed = log_normal.sample(&mut rand::thread_rng()) * 0.1;
    s.insert(id, RunningSpeed(speed));

    #[allow(clippy::cast_possible_wrap)]
    let entity_id = VarInt(id.index().0 as i32);

    let pose = event.pose;

    let pkt = play::EntitySpawnS2c {
        entity_id,
        object_uuid: uuid.0,
        kind: VarInt(EntityType::Zombie as i32),
        position: pose.position,
        pitch: ByteAngle::from_degrees(pose.pitch),
        yaw: ByteAngle::from_degrees(pose.yaw),
        head_yaw: ByteAngle(0),
        data: VarInt::default(),
        velocity: Velocity([0; 3]),
    };

    players.iter_mut().for_each(|player| {
        player.packets.writer.send_packet(&pkt).unwrap();
    });
}
