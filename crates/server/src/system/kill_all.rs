use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::VarInt;

use crate::{
    components::{MinecraftEntity, Player},
    events::KillAllEntities,
    net::{Broadcast, IoBuf},
};

#[instrument(skip_all)]
pub fn kill_all(
    _r: ReceiverMut<KillAllEntities>,
    entities: Fetcher<(EntityId, &MinecraftEntity, Not<&Player>)>,
    mut broadcast: Single<&mut Broadcast>,
    mut io: Single<&mut IoBuf>,
    mut s: Sender<Despawn>,
) {
    let ids = entities.iter().map(|(id, ..)| id).collect::<Vec<_>>();

    #[expect(clippy::cast_possible_wrap, reason = "wrapping is ok in this case")]
    let entity_ids = ids.iter().map(|id| VarInt(id.index().0 as i32)).collect();

    let despawn_packet = valence_protocol::packets::play::EntitiesDestroyS2c { entity_ids };

    broadcast.append(&despawn_packet, &mut io).unwrap();

    for id in ids {
        s.send(Despawn(id));
    }
}
