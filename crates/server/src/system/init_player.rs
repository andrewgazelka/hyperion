use evenio::prelude::*;
use tracing::instrument;

use crate::{
    net::{Connection, Encoder},
    system::entity_position::PositionSyncMetadata,
    tracker::Tracker,
    EntityReaction, FullEntityPose, InitPlayer, Player, PlayerJoinWorld, Targetable, Uuid,
};

#[instrument(skip_all)]
pub fn init_player(
    r: ReceiverMut<InitPlayer>,
    mut s: Sender<(
        Insert<FullEntityPose>,
        Insert<PositionSyncMetadata>,
        Insert<Player>,
        Insert<EntityReaction>,
        Insert<Uuid>,
        Insert<Targetable>,
        Insert<Encoder>,
        Insert<Connection>,
        PlayerJoinWorld,
    )>,
) {
    // take ownership
    let event = EventMut::take(r.event);

    let InitPlayer {
        entity,
        encoder,
        connection,
        name,
        uuid,
        pos,
    } = event;

    s.insert(entity, pos);
    s.insert(entity, Targetable);
    s.insert(entity, Player {
        last_keep_alive_sent: std::time::Instant::now(),
        unresponded_keep_alive: false,
        ping: std::time::Duration::from_secs(1),
        name,
        locale: None,
        state: Tracker::default(),
    });
    s.insert(entity, encoder);
    s.insert(entity, connection);
    s.insert(entity, Uuid(uuid));
    s.insert(entity, PositionSyncMetadata::default());

    s.insert(entity, EntityReaction::default());

    s.send(PlayerJoinWorld { target: entity });
}
