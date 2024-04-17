use evenio::prelude::*;
use tracing::{info, instrument};

use crate::{
    components::{
        AiTargetable, EntityReaction, FullEntityPose, ImmuneStatus, InGameName, KeepAlive, Player,
        Uuid, Vitals,
    },
    events::{PlayerInit, PlayerJoinWorld},
    system::entity_position::PositionSyncMetadata,
    tracker::Prev,
};

#[instrument(skip_all)]
pub fn init_player(
    r: ReceiverMut<PlayerInit>,
    mut s: Sender<(
        Insert<FullEntityPose>,
        Insert<PositionSyncMetadata>,
        Insert<Player>,
        Insert<EntityReaction>,
        Insert<Uuid>,
        Insert<ImmuneStatus>,
        Insert<Vitals>,
        Insert<Prev<Vitals>>,
        Insert<KeepAlive>,
        Insert<AiTargetable>,
        Insert<InGameName>,
        PlayerJoinWorld,
    )>,
) {
    println!("yoooooo");
    // take ownership
    let event = EventMut::take(r.event);

    // todo: bug in evenio I think where if it is targetting this event will not fire
    // let entity = event.target();

    let PlayerInit {
        entity,
        username: name,
        uuid,
        pose,
    } = event;

    info!("PlayerInit: {name}");

    s.insert(entity, pose);
    s.insert(entity, Player);
    s.insert(entity, AiTargetable);
    s.insert(entity, InGameName::from(name));
    s.insert(entity, ImmuneStatus::default());
    s.insert(entity, Uuid::from(uuid));
    s.insert(entity, PositionSyncMetadata::default());
    s.insert(entity, KeepAlive::default());

    s.insert(entity, Prev::from(Vitals::ALIVE));
    s.insert(entity, Vitals::ALIVE);

    s.insert(entity, FullEntityPose::player());
    s.insert(entity, EntityReaction::default());

    println!("sending player join world");
    s.send(PlayerJoinWorld { target: entity });
}
