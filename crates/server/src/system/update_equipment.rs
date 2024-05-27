use std::borrow::Cow;

use evenio::{
    entity::EntityId,
    event::{Receiver, Sender},
    fetch::Single,
};
use tracing::instrument;
use valence_protocol::VarInt;

use crate::{
    event::{UpdateEquipment, UpdateSelectedSlot},
    inventory::PlayerInventory,
    net::{Broadcast, Compose},
};

#[instrument(skip_all, level = "trace")]
pub fn update_main_hand(
    r: Receiver<UpdateSelectedSlot, (EntityId, &mut PlayerInventory)>,
    sender: Sender<UpdateEquipment>,
) {
    let (entity_id, inventory) = r.query;
    if inventory.set_main_hand(r.event.slot + 36).is_ok() {
        // send event
        sender.send_to(entity_id, UpdateEquipment);
    }
}

#[instrument(skip_all, level = "trace")]
pub fn update_equipment(
    r: Receiver<UpdateEquipment, (EntityId, &PlayerInventory)>,
    broadcast: Single<&Broadcast>,
    compose: Compose,
) {
    let (entity_id, inventory) = r.query;

    broadcast
        .append(
            &crate::packets::vanilla::EntityEquipmentUpdateS2c {
                entity_id: VarInt(entity_id.index().0 as i32),
                equipment: Cow::Borrowed(&inventory.get_entity_equipment()),
            },
            &compose,
        )
        .unwrap();
}
