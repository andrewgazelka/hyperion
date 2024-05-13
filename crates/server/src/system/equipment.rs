use evenio::{entity::EntityId, event::Receiver, fetch::Fetcher};
use tracing::{instrument, log::warn};
use valence_protocol::VarInt;

use crate::{
    event,
    net::{Compose, Packets},
    packets,
    packets::vanilla,
};

#[instrument(skip_all, level = "trace")]
pub fn set(
    r: Receiver<event::SetEquipment, EntityId>,
    mut players: Fetcher<(&mut Packets, EntityId)>,
    compose: Compose,
) {
    let id = r.query;
    let event = r.event;

    let pkt_self = packets::vanilla::EntityEquipmentUpdateS2c {
        entity_id: 0.into(),
        equipment: event.equipment.clone(),
    };

    // send current player
    let Ok((packet, _)) = players.get_mut(id) else {
        warn!("player not found");
        return;
    };

    packet.append(&pkt_self, &compose).unwrap();

    let pkt = vanilla::EntityEquipmentUpdateS2c {
        entity_id: VarInt(id.index().0 as i32),
        equipment: event.equipment.clone(),
    };

    // send all players
    for (packet, player_id) in &mut players {
        if player_id == id {
            continue;
        }

        packet.append(&pkt, &compose).unwrap();
    }
}
