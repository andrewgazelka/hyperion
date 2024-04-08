use std::sync::atomic::Ordering;

use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{
    packets::play,
    text::{Color, IntoText},
};

use crate::{net::Encoder, singleton::player_lookup::PlayerUuidLookup, KickPlayer, Uuid, SHARED};

#[instrument(skip_all)]
pub fn player_kick(
    r: Receiver<KickPlayer, (EntityId, &Uuid, &mut Encoder)>,
    lookup: Single<&mut PlayerUuidLookup>,
    mut s: Sender<Despawn>,
) {
    let (id, uuid, encoder) = r.query;

    lookup.0.remove(&uuid.0);

    let reason = &r.event.reason;

    let reason = reason.into_text().color(Color::RED);

    encoder
        .encode(&play::DisconnectS2c {
            reason: reason.into(),
        })
        .unwrap();

    // todo: also handle disconnecting without kicking, io socket being closed, etc
    SHARED.player_count.fetch_sub(1, Ordering::Relaxed);

    s.send(Despawn(id));
}
