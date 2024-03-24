use evenio::prelude::*;

use crate::{EntityReaction, FullEntityPose, InitPlayer, Player, PlayerJoinWorld};

pub fn init_player(
    r: ReceiverMut<InitPlayer>,
    mut s: Sender<(
        Insert<FullEntityPose>,
        Insert<Player>,
        Insert<EntityReaction>,
        PlayerJoinWorld,
    )>,
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

    s.insert(entity, EntityReaction::default());

    s.send(PlayerJoinWorld { target: entity });
}
