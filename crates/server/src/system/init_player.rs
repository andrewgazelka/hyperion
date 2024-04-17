use evenio::prelude::*;
use tracing::instrument;

use crate::{
    components::{AiTargetable, EntityReaction, FullEntityPose, InGameName, Player, Uuid},
    events::{InitPlayer, PlayerJoinWorld},
    net::LocalEncoder,
    system::entity_position::PositionSyncMetadata,
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
        Insert<AiTargetable>,
        Insert<LocalEncoder>,
        PlayerJoinWorld,
    )>,
) {
    // take ownership
    let event = EventMut::take(r.event);

    let InitPlayer {
        entity,
        name,
        uuid,
        pose,
    } = event;

    s.insert(entity, pose);
    s.insert(entity, AiTargetable);
    s.insert(entity, InGameName::from(name));
    s.insert(entity, Uuid::from(uuid));
    s.insert(entity, PositionSyncMetadata::default());
    s.insert(entity, EntityReaction::default());

    s.send(PlayerJoinWorld { target: entity });
}
