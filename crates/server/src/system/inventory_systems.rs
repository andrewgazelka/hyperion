use std::borrow::{Borrow, Cow};

use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::Fetcher,
    query::{Query, With},
};
use tracing::{instrument, warn};
use valence_protocol::{packets::play::{self, click_slot_c2s::SlotChange}, VarInt};
use valence_server::{ItemKind, ItemStack};

use crate::{
    components::{InGameName, Player},
    event::{ClickEvent, ClickType, Command},
    inventory::PlayerInventory,
    net::{Compose, Packets},
};

#[derive(Query)]
pub struct InventoryActionQuery<'a> {
    id: EntityId,
    inventory: &'a mut PlayerInventory,
    packet: &'a mut Packets,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn get_inventory_actions(r: Receiver<ClickEvent, InventoryActionQuery>, compose: Compose) {
    let click_event = r.event;

    let query = r.query;

    let ClickEvent {
        by,
        click_type,
        carried_item,
    } = click_event;

   /* match click_type {
        ClickType::LeftClick { slot, slot_change } => query.inventory.swap_carried_item(),
        // todo: implement other click types
        _ => ()
    }*/

    send_inventory_update(query.inventory, query.packet, &compose)
}

/// Sends an inventory update to the player.
fn send_inventory_update(inventory: &PlayerInventory, packet: &mut Packets, compose: &Compose) {
    let pack_inv = play::InventoryS2c {
        window_id: 0,
        state_id: VarInt(0),
        slots: Cow::Borrowed(inventory.items.get_items()),
        carried_item: Cow::Borrowed(&inventory.get_carried_item()),
    };

    packet.append(&pack_inv, &compose).unwrap();
}

#[derive(Query)]
pub struct InventoryQuery<'a> {
    name: &'a InGameName,
    inventory: &'a mut PlayerInventory,
    packet: &'a mut Packets,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn give_command(
    r: Receiver<Command, EntityId>,
    mut fetcher: Fetcher<InventoryQuery>,
    compose: Compose,
) {
    let command: &String = r.event.raw.borrow();

    if !command.starts_with("give") {
        // not a give command
        return;
    }

    let mut arguments = command.split_whitespace();

    // give <player> <item> [amount]
    let command = arguments.next();

    let player = arguments.next();

    let item = arguments.next();

    let amount = arguments.next();

    // todo make pretty when a proper command lib exists
    if let (Some(command), Some(player), Some(item), Some(amount)) = (command, player, item, amount)
    {
        if !command.eq_ignore_ascii_case("give") {
            return;
        }

        let (packet, inventory) = if let Some(x) = fetcher
            .iter_mut()
            .filter(|q| q.name.as_ref().eq(player))
            .next()
        {
            (x.packet, x.inventory)
        } else {
            warn!("give_command: player not found");
            return;
        };

        let item = ItemStack::new(
            ItemKind::from_str(item).unwrap_or(ItemKind::AcaciaBoat),
            amount.parse().unwrap_or(1),
            None,
        );

        inventory.set_first_available(item);

        send_inventory_update(inventory, packet, &compose)

        //  let (entity_id, inventory, packet) = r.query;
    } else {
        warn!("give_command: invalid command or arguments");
    }
}
