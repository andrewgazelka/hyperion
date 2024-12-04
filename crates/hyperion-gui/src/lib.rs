use std::{borrow::Cow, collections::HashMap};

use flecs_ecs::core::{Entity, EntityViewGet, World, WorldGet};
use hyperion::{
    net::{Compose, NetworkStreamRef},
    storage::GlobalEventHandlers,
    system_registry::SystemId,
    valence_protocol::{
        ItemStack, VarInt,
        packets::play::{
            // click_slot_c2s::ClickSlotC2s,
            close_screen_s2c::CloseScreenS2c,
            inventory_s2c::InventoryS2c,
            open_screen_s2c::{OpenScreenS2c, WindowType},
        },
        text::IntoText,
    },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: String,
    pub name: String,
    pub lore: Option<String>,
    pub quantity: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum ContainerType {
    Chest,
    ShulkerBox,
    Furnace,
    Dispenser,
    Hopper,
}

#[derive(Clone)]
pub struct Gui {
    items: HashMap<usize, GuiItem>,
    size: usize,
    title: String,
    window_id: u8,
    container_type: ContainerType,
}

#[derive(Clone)]
pub struct GuiItem {
    item: ItemStack,
    on_click: fn(Entity),
}

impl Gui {
    pub fn new(size: usize, title: String, container_type: ContainerType) -> Self {
        Self {
            window_id: Uuid::new_v4().as_u128() as u8,
            title,
            size,
            container_type,
            items: HashMap::new(),
        }
    }

    pub fn get_window_type(&self) -> WindowType {
        match self.container_type {
            ContainerType::Chest => WindowType::Generic9x3,
            ContainerType::ShulkerBox => WindowType::ShulkerBox,
            ContainerType::Furnace => WindowType::Furnace,
            ContainerType::Dispenser => WindowType::Generic3x3,
            ContainerType::Hopper => WindowType::Hopper,
        }
    }

    pub fn add_item(&mut self, slot: usize, item: GuiItem) -> Result<(), String> {
        if slot >= self.size {
            return Err(format!(
                "Slot {} is out of bounds for GUI of size {}",
                slot, self.size
            ));
        }

        self.items.insert(slot, item);

        Ok(())
    }

    pub fn draw<'a>(&'a self, world: &World, player: Entity) {
        let container_items: Cow<'a, [ItemStack]> = (0..self.size)
            .map(|slot| {
                self.items
                    .get(&slot)
                    .map(|gui_item| gui_item.item.clone())
                    .unwrap_or_default()
            })
            .collect();

        let binding = ItemStack::default();
        let set_content_packet = InventoryS2c {
            window_id: self.window_id,
            state_id: VarInt(0),
            slots: container_items,
            carried_item: Cow::Borrowed(&binding),
        };

        world.get::<&Compose>(|compose| {
            player
                .entity_view(world)
                .get::<&NetworkStreamRef>(|stream| {
                    compose
                        .unicast(&set_content_packet, *stream, SystemId(8), world)
                        .unwrap();
                });
        });
    }

    pub fn open<'a>(&'a mut self, world: &World, player: Entity) {
        let open_screen_packet = OpenScreenS2c {
            window_id: VarInt(self.window_id as i32),
            window_type: self.get_window_type(),
            window_title: self.title.clone().into_cow_text(),
        };

        world.get::<&Compose>(|compose| {
            player
                .entity_view(world)
                .get::<&NetworkStreamRef>(|stream| {
                    compose
                        .unicast(&open_screen_packet, *stream, SystemId(8), world)
                        .unwrap();
                });
        });

        self.draw(world, player);

        world.get::<&mut GlobalEventHandlers>(|event_handlers| {
            let window_id = self.window_id;
            let items = self.items.clone();
            let gui = self.clone();
            event_handlers.click.register(move |query, event| {
                if event.window_id != window_id {
                    return;
                }

                let slot = event.slot_idx as usize;
                let Some(item) = items.get(&slot) else {
                    return;
                };

                (item.on_click)(player);
                gui.draw(query.world, player);
            });
        });
    }

    pub fn handle_close(&mut self, _player: Entity, _close_packet: CloseScreenS2c) {
        todo!()
    }
}

impl GuiItem {
    pub fn new(item: ItemStack, on_click: fn(Entity)) -> Self {
        Self { item, on_click }
    }
}
