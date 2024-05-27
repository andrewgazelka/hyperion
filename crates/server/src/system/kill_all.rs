use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::VarInt;

use crate::{
    components::{Npc, Player},
    event::KillAllEntities,
    net::{Compose, IoBuf},
};

#[instrument(skip_all)]
pub fn kill_all(
    _r: ReceiverMut<KillAllEntities>,
    entities: Fetcher<(EntityId, &Npc, Not<&Player>)>,
    s: Sender<Despawn>,
    compose: Compose,
) {
    let ids = entities.iter().map(|(id, ..)| id).collect::<Vec<_>>();

    #[expect(clippy::cast_possible_wrap, reason = "wrapping is ok in this case")]
    let entity_ids = ids.iter().map(|id| VarInt(id.index().0 as i32)).collect();

    let despawn_packet = valence_protocol::packets::play::EntitiesDestroyS2c { entity_ids };

    // todo: use shared scratch if possible
    compose.broadcast(&despawn_packet).send().unwrap();

    for id in ids {
        s.send_to(id, Despawn);
    }
}
