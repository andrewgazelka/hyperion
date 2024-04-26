use std::sync::atomic::Ordering;

use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{
    packets::play,
    text::{Color, IntoText},
};

use crate::{
    components::Uuid,
    event::{KickPlayer, Scratch},
    global::Global,
    net::{Compressor, IoBufs, Packets},
    singleton::{player_id_lookup::EntityIdLookup, player_uuid_lookup::PlayerUuidLookup},
};

#[instrument(skip_all)]
pub fn player_kick(
    r: Receiver<KickPlayer, (EntityId, &Uuid, &mut Packets)>,
    global: Single<&Global>,
    mut uuid_lookup: Single<&mut PlayerUuidLookup>,
    mut io: Single<&mut IoBufs>,
    mut id_lookup: Single<&mut EntityIdLookup>,
    mut compressor: Single<&mut Compressor>,
    mut s: Sender<Despawn>,
) {
    let (id, uuid, packets) = r.query;

    uuid_lookup.remove(&uuid.0);
    // todo: also remove on socket close
    id_lookup.remove(&(id.index().0 as i32));

    let reason = &r.event.reason;

    let reason = reason.into_text().color(Color::RED);

    // todo: remove
    let mut scratch = Scratch::new();
    packets
        .append(
            &play::DisconnectS2c {
                reason: reason.into(),
            },
            io.one(),
            &mut scratch,
            compressor.one(),
        )
        .unwrap();

    // todo: also handle disconnecting without kicking, io socket being closed, etc

    global.0.shared.player_count.fetch_sub(1, Ordering::Relaxed);

    s.send(Despawn(id));
}
