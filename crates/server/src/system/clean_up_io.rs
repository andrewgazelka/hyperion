use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{packets::play, VarInt};

use crate::{
    singleton::encoder::{Encoder, PacketMetadata},
    Gametick, Player, Uuid,
};

#[instrument(skip_all, level = "trace")]
pub fn clean_up_io(
    _r: Receiver<Gametick>,
    io_entities: Fetcher<(EntityId, &mut Player, &Uuid)>,
    encoder: Single<&mut Encoder>,

    mut s: Sender<Despawn>,
) {
    // todo: this might not be that efficient honestly
    let mut despawn_uuids = Vec::new();
    let mut despawn_ids = Vec::new();

    for (id, player, uuid) in io_entities {
        if !player.packets.writer.is_closed() {
            continue;
        }

        s.send(Despawn(id));
        despawn_uuids.push(uuid.0);

        let id_raw = id.index().0;
        let id_raw = VarInt(id_raw as i32);

        despawn_ids.push(id_raw);
    }

    let encoder = encoder.0.get_round_robin();

    encoder
        .append_packet(
            &play::EntitiesDestroyS2c {
                entity_ids: despawn_ids.into(),
            },
            PacketMetadata::REQUIRED,
        )
        .unwrap();

    encoder
        .append_packet(
            &play::PlayerRemoveS2c {
                uuids: despawn_uuids.into(),
            },
            PacketMetadata::REQUIRED,
        )
        .unwrap();
}
