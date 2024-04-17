use evenio::prelude::*;
use tracing::instrument;

use crate::{
    net::Encoder, system::entity_position::PositionSyncMetadata, tracker::Tracker, EntityReaction,
    FullEntityPose, InitPlayer, Player, PlayerJoinWorld, Targetable, Uuid,
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
        PlayerJoinWorld,
    )>,
) {
    // take ownership
    let event = EventMut::take(r.event);

    let InitPlayer {
        entity,
        encoder,
        name,
        uuid,
        pos,
    } = event;

    s.insert(entity, pos);
    s.insert(entity, Targetable);
    s.insert(entity, Player {
        last_keep_alive_sent: std::time::Instant::now(),
        unresponded_keep_alive: false,
        name,
        locale: None,
        state: Tracker::default(),
        immune_until: 0,
    });
    s.insert(entity, encoder);
    s.insert(entity, Uuid(uuid));
    s.insert(entity, PositionSyncMetadata::default());

    s.insert(entity, EntityReaction::default());

    s.send(PlayerJoinWorld { target: entity });
}
