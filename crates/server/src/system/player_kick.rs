use std::sync::atomic::Ordering;

use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{
    packets::play,
    text::{Color, IntoText},
};

use crate::{
    global::Global,
    net::Encoder,
    singleton::{player_id_lookup::PlayerIdLookup, player_uuid_lookup::PlayerUuidLookup},
    KickPlayer, Uuid,
};

#[instrument(skip_all)]
pub fn player_kick(
    r: Receiver<KickPlayer, (EntityId, &Uuid, &mut Encoder)>,
    global: Single<&Global>,
    mut uuid_lookup: Single<&mut PlayerUuidLookup>,
    mut id_lookup: Single<&mut PlayerIdLookup>,
    mut s: Sender<Despawn>,
) {
    let (id, uuid, encoder) = r.query;

    uuid_lookup.remove(&uuid.0);
    // todo: also remove on socket close
    id_lookup.inner.remove(&(id.index().0 as i32));

    let reason = &r.event.reason;

    let reason = reason.into_text().color(Color::RED);

    encoder
        .encode(&play::DisconnectS2c {
            reason: reason.into(),
        })
        .unwrap();

    // todo: also handle disconnecting without kicking, io socket being closed, etc

    global.0.shared.player_count.fetch_sub(1, Ordering::Relaxed);

    s.send(Despawn(id));
}
