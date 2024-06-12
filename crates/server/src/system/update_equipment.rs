use std::borrow::Cow;

use evenio::{
    entity::EntityId,
    event::{Receiver, Sender},
};
use tracing::instrument;
use valence_protocol::VarInt;

use crate::{
    event::{UpdateEquipment, UpdateSelectedSlot},
    inventory::PlayerInventory,
    net::Compose,
};

#[instrument(skip_all, level = "trace")]
pub fn update_main_hand(
    r: Receiver<UpdateSelectedSlot, (EntityId, &mut PlayerInventory)>,
    sender: Sender<UpdateEquipment>,
) {
    let (entity_id, inventory) = r.query;
    if inventory.set_main_hand_slot(r.event.slot + 36).is_ok() {
        // send event
        sender.send_to(entity_id, UpdateEquipment);
    }
}

#[instrument(skip_all, level = "trace")]
pub fn update_equipment(
    r: Receiver<UpdateEquipment, (EntityId, &PlayerInventory)>,
    compose: Compose,
) {
    let (entity_id, inventory) = r.query;

    compose
        .broadcast(&crate::packets::vanilla::EntityEquipmentUpdateS2c {
            entity_id: VarInt(entity_id.index().0 as i32),
            equipment: Cow::Borrowed(&inventory.get_entity_equipment()),
        })
        .send()
        .unwrap();
}
