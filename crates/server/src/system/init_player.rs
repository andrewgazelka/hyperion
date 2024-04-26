use std::borrow::Cow;

use anyhow::Context;
use evenio::prelude::*;
use sha2::Digest;
use tracing::{info, instrument};
use valence_protocol::{packets::login, Bounded};

use crate::{
    components::{
        AiTargetable, EntityReaction, FullEntityPose, ImmuneStatus, InGameName, KeepAlive, Player,
        Uuid, Vitals,
    },
    event::{PlayerInit, PlayerJoinWorld, Scratch},
    net::{Compressor, IoBufs, Packets},
    system::sync_entity_position::PositionSyncMetadata,
    tracker::Prev,
};

/// Get a [`uuid::Uuid`] based on the given user's name.
fn offline_uuid(username: &str) -> anyhow::Result<uuid::Uuid> {
    let digest = sha2::Sha256::digest(username);

    #[expect(clippy::indexing_slicing, reason = "sha256 is always 32 bytes")]
    let slice = &digest[..16];

    uuid::Uuid::from_slice(slice).context("failed to create uuid")
}

#[instrument(skip_all)]
pub fn init_player(
    r: ReceiverMut<PlayerInit, &Packets>,
    mut io: Single<&mut IoBufs>,
    mut compressor: Single<&mut Compressor>,
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
    // take ownership
    let event = EventMut::take(r.event);

    // todo: bug in evenio I think where if it is targeting this event will not fire
    // let entity = event.target();

    let PlayerInit {
        entity,
        username,
        pose,
    } = event;

    let uuid = offline_uuid(&username).unwrap();

    let pkt = login::LoginSuccessS2c {
        uuid,
        username: Bounded(&username),
        properties: Cow::default(),
    };

    let packets = r.query;

    let mut scratch = Scratch::new();
    packets
        .append(&pkt, io.one(), &mut scratch, compressor.one())
        .unwrap();

    info!("PlayerInit: {username}");

    s.insert(entity, pose);
    s.insert(entity, Player);
    s.insert(entity, AiTargetable);
    s.insert(entity, InGameName::from(username));
    s.insert(entity, ImmuneStatus::default());
    s.insert(entity, Uuid::from(uuid));
    s.insert(entity, PositionSyncMetadata::default());
    s.insert(entity, KeepAlive::default());

    s.insert(entity, Prev::from(Vitals::ALIVE));
    s.insert(entity, Vitals::ALIVE);

    s.insert(entity, FullEntityPose::player());
    s.insert(entity, EntityReaction::default());

    s.send(PlayerJoinWorld { target: entity });
}
