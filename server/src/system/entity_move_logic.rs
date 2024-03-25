#![allow(unused)]

use std::process::id;

use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::Fetcher,
    query::{Has, Not, Query, With},
    rayon::prelude::*,
};
use tracing::instrument;
use valence_protocol::{
    math::{DVec2, DVec3},
    ByteAngle, VarInt,
};

use crate::{
    io::encode_packet, EntityReaction, FullEntityPose, Gametick, MinecraftEntity, Player,
    RunningSpeed, Targetable,
};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    id: EntityId,
    running_speed: Option<&'a RunningSpeed>,
    reaction: &'a mut EntityReaction,
    pose: &'a mut FullEntityPose,
    _entity: With<&'static MinecraftEntity>,
}

#[instrument(skip_all, name = "entity_move_logic")]
pub fn entity_move_logic(
    _: Receiver<Gametick>,
    mut entities: Fetcher<(With<&MinecraftEntity>, &mut FullEntityPose)>,
    mut target: Fetcher<(&Targetable, Not<&MinecraftEntity>, &FullEntityPose)>,
    mut player: Fetcher<(&Player, Not<&MinecraftEntity>, &FullEntityPose)>,
) {
    use valence_protocol::packets::play;
    
    let Some((_, _, &target)) = target.iter_mut().next() else {
        // no movement if not a single player
        return;
    };
    
    let target = target.position;
    
    entities.par_iter_mut().for_each(|query| {
        let EntityQuery {
            id,
            running_speed,
            pose,
            reaction,
            ..
        } = query;
    
        let current = pose.position;
    
        let dif = target - current;
    
        let dif2d = DVec2::new(dif.x, dif.z);
    
        let yaw = dif2d.y.atan2(dif2d.x).to_degrees() as f32;
    
        // subtract 90 degrees
        let yaw = yaw - 90.0;
    
        let pitch = 0.0;
    
        let reaction = reaction.get_mut();
    
        if dif2d.length_squared() < 0.01 {
            // info!("Moving entity {:?} by {:?}", id, reaction.velocity);
            pose.move_by(reaction.velocity);
        } else {
            // normalize
            let dif2d = dif2d.normalize();
    
            let speed = running_speed.copied().unwrap_or_default();
            let dif2d = dif2d * speed.0;
    
            let vec = DVec3::new(dif2d.x, 0.0, dif2d.y) + reaction.velocity;
    
            pose.move_by(vec);
        }
    
        reaction.velocity = DVec3::ZERO;
    
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
    
        // todo: remove unwrap
        let pos = encode_packet(&pos).unwrap();
        let look = encode_packet(&look).unwrap();
    
        player.iter().for_each(|(player, ..)| {
            let _ = player.packets.writer.send_raw(pos.clone());
            let _ = player.packets.writer.send_raw(look.clone());
        });
    });
}
