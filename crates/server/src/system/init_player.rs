use std::borrow::Cow;

use anyhow::Context;
use evenio::prelude::*;
use sha2::Digest;
use tracing::{instrument, trace};
use valence_protocol::{packets::login, Bounded};

use crate::{
    components::{
        AiTargetable, ChunkLocation, EntityReaction, FullEntityPose, ImmuneStatus, InGameName,
        KeepAlive, Player, Uuid, Vitals,
    },
    event::{PlayerInit, PlayerJoinWorld},
    inventory::PlayerInventory,
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
    r: ReceiverMut<PlayerInit, (EntityId, &mut Packets)>,
    compose: Compose,
    s: Sender<(
        Insert<FullEntityPose>,
        Insert<PositionSyncMetadata>,
        Insert<Player>,
        Insert<EntityReaction>,
        Insert<Uuid>,
        Insert<ImmuneStatus>,
        Insert<Vitals>,
        Insert<Prev<Vitals>>,
        Insert<KeepAlive>,
        Insert<ChunkLocation>,
        Insert<AiTargetable>,
        Insert<InGameName>,
        Insert<ChunkChanges>,
        Insert<PlayerInventory>,
        PlayerJoinWorld,
    )>,
) {
    // take ownership
    let event = EventMut::take(r.event);

    let PlayerInit { username, pose } = event;

    let (id, packets) = r.query;

    let uuid = offline_uuid(&username).unwrap();

    let pkt = login::LoginSuccessS2c {
        uuid,
        username: Bounded(&username),
        properties: Cow::default(),
    };

    compose.unicast(&pkt, *packets).unwrap();

    trace!("PlayerInit: {username}");

    s.insert(id, pose);
    s.insert(id, Player);
    s.insert(id, AiTargetable);
    s.insert(id, InGameName::from(username));
    s.insert(id, ImmuneStatus::default());
    s.insert(id, Uuid::from(uuid));
    s.insert(id, PositionSyncMetadata::default());
    s.insert(id, KeepAlive::default());

    s.insert(id, Prev::from(Vitals::ALIVE));
    s.insert(id, Vitals::ALIVE);
    s.insert(id, PlayerInventory::new());

    s.insert(id, FullEntityPose::player());
    s.insert(id, ChunkChanges::default());

    // so we always send updates
    s.insert(id, ChunkLocation::NULL);

    s.insert(id, EntityReaction::default());

    s.send_to(id, PlayerJoinWorld);
}
