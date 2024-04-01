use evenio::{prelude::*, rayon::prelude::*};
use tracing::instrument;
use valence_protocol::VarInt;

use crate::{KillAllEntities, MinecraftEntity, Player};

#[instrument(skip_all)]
pub fn kill_all(
    _r: ReceiverMut<KillAllEntities>,
    entities: Fetcher<(EntityId, &MinecraftEntity, Not<&Player>)>,
    mut players: Fetcher<&mut Player>,
    mut s: Sender<Despawn>,
) {
    let ids = entities.iter().map(|(id, ..)| id).collect::<Vec<_>>();

    #[expect(clippy::cast_possible_wrap, reason = "wrapping is ok in this case")]
    let entity_ids = ids.iter().map(|id| VarInt(id.index().0 as i32)).collect();

    let despawn_packet = valence_protocol::packets::play::EntitiesDestroyS2c { entity_ids };

    players.par_iter_mut().for_each(|player| {
        // todo: handle error
        let _ = player.packets.writer.send_packet(&despawn_packet);
    });

    for id in ids {
        s.send(Despawn(id));
    }
}
