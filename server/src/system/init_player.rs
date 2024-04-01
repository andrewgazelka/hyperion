use evenio::prelude::*;
use tracing::instrument;

use crate::{
    EntityReaction, FullEntityPose, InitPlayer, Player, PlayerJoinWorld, Targetable, Uuid,
};

#[instrument(skip_all)]
pub fn init_player(
    r: ReceiverMut<InitPlayer>,
    mut s: Sender<(
        Insert<FullEntityPose>,
        Insert<Player>,
        Insert<EntityReaction>,
        Insert<Uuid>,
        Insert<Targetable>,
        PlayerJoinWorld,
    )>,
) {
    // take ownership
    let event = EventMut::take(r.event);

    let InitPlayer {
        entity,
        io,
        name,
        uuid,
        pos,
    } = event;

    s.insert(entity, pos);
    s.insert(entity, Targetable);
    s.insert(entity, Player {
        packets: io,
        last_keep_alive_sent: std::time::Instant::now(),
        unresponded_keep_alive: false,
        ping: std::time::Duration::from_secs(1),
        name,
        locale: None,
    });
    s.insert(entity, Uuid(uuid));

    s.insert(entity, EntityReaction::default());

    s.send(PlayerJoinWorld { target: entity });
}
