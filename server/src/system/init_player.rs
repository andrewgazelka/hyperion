use evenio::prelude::*;

use crate::{FullEntityPose, InitPlayer, Player, PlayerJoinWorld};

pub fn init_player(
    r: ReceiverMut<InitPlayer>,
    mut s: Sender<(Insert<FullEntityPose>, Insert<Player>, PlayerJoinWorld)>,
) {
    // take ownership
    let event = EventMut::take(r.event);

    let InitPlayer {
        entity,
        io,
        name,
        pos,
    } = event;

    s.insert(entity, pos);
    s.insert(entity, Player {
        packets: io,
        last_keep_alive_sent: std::time::Instant::now(),
        name,
        locale: None,
    });

    s.send(PlayerJoinWorld { target: entity });
}
