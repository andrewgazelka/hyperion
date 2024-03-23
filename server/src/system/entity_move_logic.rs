#![allow(unused)]

use evenio::{entity::EntityId, event::Receiver, fetch::Fetcher, query::Not};
use tracing::info;
use valence_protocol::{math::DVec2, ByteAngle, VarInt};

use crate::{FullEntityPose, Gametick, Player, Zombie};

pub fn call(
    _: Receiver<Gametick>,
    mut zombies: Fetcher<(&Zombie, Not<&mut Player>, EntityId, &mut FullEntityPose)>,
    mut player: Fetcher<(&mut Player, Not<&Zombie>, &FullEntityPose)>,
) {
    use valence_protocol::packets::play;

    let Some((_, _, &target)) = player.iter_mut().next() else {
        // no movement if not a single player
        return;
    };

    let target = target.position;

    // info!("tick");

    zombies.iter_mut().for_each(|(_, _, id, pose)| {
        let current = pose.position;

        // info!("zombie: {:?} target: {:?}", current, target);

        let dif = target - current;

        let dif2d = DVec2::new(dif.x, dif.z);

        let yaw = dif2d.y.atan2(dif2d.x).to_degrees() as f32;

        // subtract 90 degrees
        let yaw = yaw - 90.0;

        let pitch = 0.0;

        if dif2d.length_squared() < 0.1 {
            return;
        }

        // normalize
        let dif2d = dif2d.normalize();

        // make 0.1
        let dif2d = dif2d * 0.1;

        pose.position.x += dif2d.x;
        pose.position.z += dif2d.y;

        #[allow(clippy::cast_possible_wrap)]
        let entity_id = VarInt(id.index().0 as i32);

        let pos = play::EntityPositionS2c {
            entity_id,
            position: pose.position,
            yaw: ByteAngle::from_degrees(yaw),
            pitch: ByteAngle::from_degrees(pitch),
            on_ground: false,
        };

        let look = play::EntitySetHeadYawS2c {
            entity_id,
            head_yaw: ByteAngle::from_degrees(yaw),
        };

        player.iter_mut().for_each(|(player, ..)| {
            // todo: this is inefficient we want to serialize once
            // todo: remove _
            let _ = player.packets.writer.send_packet(&pos);
            let _ = player.packets.writer.send_packet(&look);
        });
    });
}
