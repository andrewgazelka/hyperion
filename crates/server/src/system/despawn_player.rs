use std::borrow::Cow;

use evenio::prelude::*;
use tracing::{info, instrument};
use valence_protocol::{packets::play, VarInt};

use crate::{
    components::{InGameName, Uuid},
    global::Global,
    net::{Compose, IoBuf},
};

#[instrument(skip_all, level = "trace")]
pub fn despawn_player(
    r: Receiver<Despawn, (&Uuid, &InGameName, EntityId)>,
    compose: Compose,
    global: Single<&Global>,
) {
    let (uuid, name, id) = r.query;

    let uuid = uuid.0;
    let uuids = &[uuid];

    let id = id.index().0 as i32;
    let entity_ids = &[VarInt(id)];

    // destroy
    let pkt = play::EntitiesDestroyS2c {
        entity_ids: Cow::Borrowed(entity_ids),
    };

    compose.broadcast(&pkt).send().unwrap();

    let pkt = play::PlayerRemoveS2c {
        uuids: uuids.into(),
    };

    compose.broadcast(&pkt).send().unwrap();

    info!("{name} disconnected");

    global
        .0
        .shared
        .player_count
        .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
}
