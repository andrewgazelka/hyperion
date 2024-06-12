use std::{borrow::Cow, time::SystemTime};

use anyhow::{bail, Context};
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
    event,
    event::{ChatMessage, ClickEvent, Command, ItemInteract, UpdateEquipment},
    inventory::{Interaction, PlayerInventory},
    net::{Compose, StreamId},
};

#[instrument(skip_all, level = "trace")]
pub fn item_interact(
    r: Receiver<ItemInteract, InventoryActionQuery>,
    _compose: Compose,
    _sender: Sender<UpdateEquipment>,
) {
    r.query.inventory.interaction = Some(Interaction {
        start: SystemTime::now(),
        hand: r.event.hand,
    });
}

#[derive(Query)]
pub struct InventoryActionQuery<'a> {
    id: EntityId,
    inventory: &'a mut PlayerInventory,
    packet: &'a mut StreamId,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn get_inventory_actions(
    r: Receiver<ClickEvent, InventoryActionQuery>,
    compose: Compose,
    sender: Sender<UpdateEquipment>,
) {
    let click_event = r.event;

    let query = r.query;

    let ClickEvent {
        carried_item,
        slot_changes,
        click_type: _,
    } = click_event;

    match query
        .inventory
        .try_append_changes(slot_changes, carried_item, false)
    {
        Ok(result) if result.update_equipment => sender.send_to(query.id, UpdateEquipment),
        // error must not be handled, the server resets the inventory
        _ => (),
    }
    send_inventory_update(query.inventory, query.packet, &compose);
}

/// Sends an inventory update to the player.
fn send_inventory_update(inventory: &PlayerInventory, packet: &mut StreamId, compose: &Compose) {
    let pack_inv = play::InventoryS2c {
        window_id: 0,
        state_id: VarInt(0),
        slots: Cow::Borrowed(inventory.items.get_items()),
        carried_item: Cow::Borrowed(inventory.get_carried_item()),
    };

    compose.unicast(&pack_inv, packet).unwrap();
}

pub fn update_inventory(
    r: Receiver<event::UpdateInventory, (&PlayerInventory, &mut StreamId)>,
    compose: Compose,
) {
    let (inventory, stream) = r.query;

    let pack_inv = play::InventoryS2c {
        window_id: 0,
        state_id: VarInt(0),
        slots: Cow::Borrowed(inventory.items.get_items()),
        carried_item: Cow::Borrowed(inventory.get_carried_item()),
    };

    compose.unicast(&pack_inv, stream).unwrap();
}

#[derive(Query)]
pub struct InventoryQuery<'a> {
    name: &'a InGameName,
    inventory: &'a mut PlayerInventory,
    packet: &'a mut StreamId,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn give_command(
    r: Receiver<Command, EntityId>,
    mut fetcher: Fetcher<InventoryQuery>,
    compose: Compose,
    sender: Sender<ChatMessage>,
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

        let amount = match arguments.next() {
            Some(amount) => amount.parse().context("expected amount to be a number")?,
            None => 1,
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

        let item = ItemStack::new(item, amount, None);

        inventory.fit(item);

        send_inventory_update(inventory, packet, &compose);
        Ok(())
    };

    if let Err(err) = inner() {
        sender.send_to(id, ChatMessage::new(err.to_string().into_cow_text()));
    }
}
