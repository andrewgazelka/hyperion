use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{
    packets::{play, play::player_position_look_s2c::PlayerPositionLookFlags},
    VarInt,
};

use crate::{
    event,
    event::Scratch,
    net::{Compressor, IoBufs, Packets},
};

#[derive(Query)]
pub struct TeleportQuery<'a> {
    packets: &'a mut Packets,
}

#[instrument(skip_all)]
pub fn teleport(
    r: Receiver<event::Teleport, TeleportQuery>,
    mut io: Single<&mut IoBufs>,
    mut compressor: Single<&mut Compressor>,
) {
    // todo: other players should see this instantly. we need to figure out the best way to do this.
    // don't want to make it seem like the player is cheating when they are not and just have not gotten
    // update yet.
    // To do this, we shuold probably use teleport_id correctly.
    let event = r.event;
    let query = r.query;

    let mut scratch = Scratch::new();

    // PlayerPositionLookS2CPacket

    let teleport_id = fastrand::i32(..);
    let teleport_id = VarInt(teleport_id);
    
    query
        .packets
        .append(
            &play::PlayerPositionLookS2c {
                position: event.position.as_dvec3(),
                yaw: 0.0,
                pitch: 0.0,
                flags: PlayerPositionLookFlags::default(),
                teleport_id,
            },
            io.one(),
            &mut scratch,
            compressor.one(),
        )
        .unwrap();
}
