use evenio::prelude::*;

use crate::{FullEntityPose, InitPlayer, Player, PlayerJoinWorld};

pub fn init_player(
    r: ReceiverMut<InitPlayer>,
    mut s: Sender<(Insert<FullEntityPose>, Insert<Player>, PlayerJoinWorld)>,
) {
    // take ownership
    let event = EventMut::take(r.event);

    let InitPlayer { entity, io, pos } = event;

    s.insert(entity, pos);
    s.insert(entity, Player {
        packets: io,
        locale: None,
    });

    s.send(PlayerJoinWorld { target: entity });
}
