use std::borrow::Cow;

use anyhow::Context;
use evenio::prelude::*;
use sha2::Digest;
use tracing::{instrument, trace};
use valence_protocol::{packets::login, Bounded};

use crate::{
    components::{
        AiTargetable, EntityReaction, FullEntityPose, ImmuneStatus, InGameName, KeepAlive,
        LastSentChunk, Player, Uuid, Vitals,
    },
    event::{PlayerInit, PlayerJoinWorld},
    net::{Compose, Packets},
    system::{chunks::ChunkChanges, sync_entity_position::PositionSyncMetadata},
    tracker::Prev,
};

/// Get a [`uuid::Uuid`] based on the given user's name.
fn offline_uuid(username: &str) -> anyhow::Result<uuid::Uuid> {
    let digest = sha2::Sha256::digest(username);

    #[expect(clippy::indexing_slicing, reason = "sha256 is always 32 bytes")]
    let slice = &digest[..16];

    uuid::Uuid::from_slice(slice).context("failed to create uuid")
}

#[instrument(skip_all, level = "trace")]
pub fn init_player(
    r: ReceiverMut<PlayerInit, &mut Packets>,
    compose: Compose,
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
        Insert<LastSentChunk>,
        Insert<AiTargetable>,
        Insert<InGameName>,
        Insert<ChunkChanges>,
        PlayerJoinWorld,
    )>,
) {
    // take ownership
    let event = EventMut::take(r.event);

    let PlayerInit {
        target: entity,
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

    packets.append(&pkt, &compose).unwrap();

    trace!("PlayerInit: {username}");

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
    s.insert(entity, ChunkChanges::default());

    // so we always send updates
    s.insert(entity, LastSentChunk::NULL);

    s.insert(entity, EntityReaction::default());

    s.send(PlayerJoinWorld { target: entity });
}
