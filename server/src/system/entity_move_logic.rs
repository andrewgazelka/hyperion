use evenio::{entity::EntityId, event::Receiver, fetch::Fetcher};
use valence_protocol::{ByteAngle, VarInt};

use crate::{FullEntityPose, Gametick, Player, Zombie};

pub fn call(
    _: Receiver<Gametick>,
    mut zombies: Fetcher<(&Zombie, EntityId, &mut FullEntityPose)>,
    mut player: Fetcher<&mut Player>,
) {
    use valence_protocol::packets::play;
    zombies.iter_mut().for_each(|(_, id, pose)| {
        pose.position.x += 0.1;

        #[allow(clippy::cast_possible_wrap)]
        let entity_id = VarInt(id.index().0 as i32);

        let pkt = play::EntityPositionS2c {
            entity_id,
            position: pose.position,
            yaw: ByteAngle::from_degrees(pose.yaw),
            pitch: ByteAngle::from_degrees(pose.pitch),
            on_ground: false,
        };

        player.iter_mut().for_each(|player| {
            // todo: this is inefficient we want to serialize once
            // todo: remove _
            let _ = player.packets.writer.send_packet(&pkt);
        });
    });
}
