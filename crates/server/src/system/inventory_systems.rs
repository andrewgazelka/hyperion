use std::borrow::Cow;

use anyhow::bail;
use evenio::{
    entity::EntityId,
    event::{Receiver, Sender},
    fetch::Fetcher,
    query::{Query, With},
};
use tracing::instrument;
use valence_protocol::{
    packets::play::{self},
    VarInt,
};
use valence_server::{ItemKind, ItemStack};
use valence_text::IntoText;

use crate::{
    components::{InGameName, Player},
    event::{ChatMessage, ClickEvent, Command, UpdateEquipment},
    inventory::PlayerInventory,
    net::{Compose, Packets},
};

#[derive(Query)]
pub struct InventoryActionQuery<'a> {
    _id: EntityId,
    inventory: &'a mut PlayerInventory,
    packet: &'a mut Packets,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn get_inventory_actions(
    r: Receiver<ClickEvent, InventoryActionQuery>,
    compose: Compose,
    mut sender: Sender<UpdateEquipment>,
) {
    let click_event = r.event;

    let query = r.query;

    let ClickEvent {
        by,
        carried_item,
        slot_changes,
        click_type: _,
    } = click_event;

    match query
        .inventory
        .append_slot_changes(slot_changes, carried_item, false)
    {
        Ok(result) if result.update_equipment => sender.send(UpdateEquipment { id: *by }),
        // error must not be handled, the server resets the inventory
        _ => (),
    }
    send_inventory_update(query.inventory, query.packet, &compose);
}

/// Sends an inventory update to the player.
fn send_inventory_update(inventory: &PlayerInventory, packet: &mut Packets, compose: &Compose) {
    let pack_inv = play::InventoryS2c {
        window_id: 0,
        state_id: VarInt(0),
        slots: Cow::Borrowed(inventory.items.get_items()),
        carried_item: Cow::Borrowed(inventory.get_carried_item()),
    };

    packet.append(&pack_inv, compose).unwrap();
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
    mut sender: Sender<ChatMessage>,
) {
    let id = r.query;
    let mut inner = || -> anyhow::Result<()> {
        let command = &r.event.raw;

        if !command.starts_with("give") {
            // not a give command
            return Ok(());
        }

        let mut arguments = command.split_whitespace();

        let format = "give <player> <item> [amount]";

        // give <player> <item> [amount]
        let Some(_) = arguments.next() else {
            bail!("expected command to be {format}");
        };

        let Some(player) = arguments.next() else {
            bail!("expected player to be /give §c<player> <item> [amount]");
        };

        let Some(item) = arguments.next() else {
            bail!("expected item to be /give <player> §c<item> [amount]");
        };

        let Some(amount) = arguments.next() else {
            bail!("expected amount to be /give <player> <item> §c[amount]");
        };

        let (packet, inventory) =
            if let Some(x) = fetcher.iter_mut().find(|q| q.name.as_ref() == player) {
                (x.packet, x.inventory)
            } else {
                bail!("give_command: player not found");
            };

        // remove prefix `minecraft:` if it exists
        let item = item.strip_prefix("minecraft:").unwrap_or(item);

        let Some(item) = ItemKind::from_str(item) else {
            bail!("give_command: invalid item {item}");
        };

        let item = ItemStack::new(item, amount.parse().unwrap_or(1), None);

        inventory.set_first_available(item);

        send_inventory_update(inventory, packet, &compose);
        Ok(())
    };

    if let Err(err) = inner() {
        sender.send(ChatMessage::new(id, err.to_string().into_cow_text()));
    }
}
