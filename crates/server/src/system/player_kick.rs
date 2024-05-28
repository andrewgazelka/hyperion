use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{
    packets::play,
    text::{Color, IntoText},
};

use crate::{
    components::Uuid,
    event::KickPlayer,
    net::{Compose, StreamId},
    singleton::{player_id_lookup::EntityIdLookup, player_uuid_lookup::PlayerUuidLookup},
};

#[instrument(skip_all)]
pub fn player_kick(
    r: Receiver<KickPlayer, (EntityId, &Uuid, &mut StreamId)>,
    mut uuid_lookup: Single<&mut PlayerUuidLookup>,
    mut id_lookup: Single<&mut EntityIdLookup>,
    compose: Compose,
    s: Sender<Despawn>,
) {
    let (id, uuid, packets) = r.query;

    uuid_lookup.remove(&uuid.0);
    // todo: also remove on socket close
    id_lookup.remove(&(id.index().0 as i32));

    let reason = &r.event.reason;

    let reason = reason.into_text().color(Color::RED);
    let reason = reason.into();
    compose
        .unicast(&play::DisconnectS2c { reason }, *packets)
        .unwrap();

    s.send_to(id, Despawn);
}
