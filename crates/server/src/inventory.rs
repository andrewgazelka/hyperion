use std::{borrow::BorrowMut, mem, ops::RangeInclusive};

use evenio::component::Component;
use itertools::Either;
use valence_protocol::packets::play::{
    click_slot_c2s::SlotChange, entity_equipment_update_s2c::EquipmentEntry,
};
use valence_server::ItemStack;

#[derive(Debug)]
pub struct Inventory<const T: usize> {
    pub slots: [ItemStack; T],
}

impl<const T: usize> Inventory<T> {
    /// Constructs a new inventory with the given size.
    fn new() -> Self {
        Self {
            slots: [ItemStack::EMPTY; T],
        }
    }

    /// Set the item stack at the given index.
    pub fn set(&mut self, index: usize, item: ItemStack) {
        self.slots[index] = item;
    }

    /// Set item at first available spot
    /// Returns Left if the item was placed or the item if no spot was found
    pub fn set_first_available(
        &mut self,
        range: RangeInclusive<usize>,
        item: ItemStack,
    ) -> Either<(), ItemStack> {
        let hotbar = &mut self.slots[range];
        if let Some(found_slot) = hotbar
            .iter_mut()
            .filter(|e| e.is_empty())
            .enumerate()
            .min_by_key(|(number, _)| *number)
            .map(|(_, item)| item)
        {
            *found_slot = item;
            return Either::Left(());
        }
        Either::Right(item)
    }

    /// remove item at index
    pub fn remove(&mut self, index: usize) -> ItemStack {
        let item = mem::replace(&mut self.slots[index], ItemStack::EMPTY);
        item
    }

    /// Get all items in the inventory
    /// to send to client
    pub fn get_items(&self) -> &[ItemStack; T] {
        &self.slots
    }
}

/// The player's inventory.
#[derive(Component, Debug)]
pub struct PlayerInventory {
    /// Items held by player
    pub items: Inventory<46>,
    /// main hand index
    main_hand: usize,
    /// carried item
    ///
    /// This item will be none when player closes inventory
    carried_item: ItemStack,
}

impl PlayerInventory {
    /// Constructs a new player inventory.
    pub fn new() -> Self {
        Self {
            items: Inventory::new(),
            main_hand: 36,
            carried_item: ItemStack::EMPTY,
        }
    }

    /// Swap the carried item with the item at the given index.
    /// If the item in the slot is the same as the carried item, the items will stack. the rest is left in the carried item
    /// It also checks if the change is within the changes the client sent
    /// This is called when left clicking a slot
    pub fn swap_or_stack_carried_item(
        &mut self,
        index: usize,
        clients_carried_item: ItemStack,
        clients_item_change: SlotChange,
    ) -> Result<(), ()> {
        let item = match self.items.slots.get_mut(index) {
            Some(item) => item,
            None => return Err(()),
        };

        mem::swap(item, &mut self.carried_item);

        Ok(())
    }

    /// stack the carried item with the item at the given index.

    /// Swap the carried item with the item at the given index.
    pub fn swap_carried_item(&mut self, index: usize) -> Result<(), ()> {
        let item = match self.items.slots.get_mut(index) {
            Some(item) => item,
            None => return Err(()),
        };

        mem::swap(item, &mut self.carried_item);
        Ok(())
    }

    /// Get the Carried Item
    pub fn get_carried_item(&self) -> &ItemStack {
        &self.carried_item
    }

    /// Set item at first available spot
    pub fn set_first_available(&mut self, item: ItemStack) -> Either<(), ItemStack> {
        // try hotbar
        let item = match self.items.set_first_available(36..=44, item) {
            Either::Left(_) => return Either::Left(()),
            Either::Right(item) => item,
        };

        // try inventory
        let item = match self.items.set_first_available(9..=35, item) {
            Either::Left(_) => return Either::Left(()),
            Either::Right(item) => item,
        };

        // return item because no spot was found
        Either::Right(item)
    }

    /// set item in the offhand
    pub fn set_offhand(&mut self, item: ItemStack) {
        self.items.set(45, item);
    }

    /// get item in the offhand
    pub fn get_offhand(&self) -> Option<&ItemStack> {
        self.items.slots.get(45)
    }

    /// get item in the main hand
    pub fn get_main_hand(&self) -> Option<&ItemStack> {
        self.items.slots.get(self.main_hand)
    }

    /// set main hand index to
    pub fn set_main_hand(&mut self, index: usize) -> Result<(), ()> {
        if index < 36 || index > 44 {
            // main hand can only be in the hotbar
            return Err(());
        }
        self.main_hand = index;
        Ok(())
    }

    /// set helmet slot 5
    pub fn set_helmet(&mut self, item: ItemStack) {
        // todo check if item is helmet
        self.items.set(5, item);
    }

    /// get helmet slot 5
    pub fn get_helmet(&self) -> Option<&ItemStack> {
        self.items.slots.get(5)
    }

    /// set chestplate slot 6
    pub fn set_chestplate(&mut self, item: ItemStack) {
        // todo check if item is chestplate
        self.items.set(6, item);
    }

    /// get chestplate slot 6
    pub fn get_chestplate(&self) -> Option<&ItemStack> {
        self.items.slots.get(6)
    }

    /// set leggings slot 7
    pub fn set_leggings(&mut self, item: ItemStack) {
        // todo check if item is leggings
        self.items.set(7, item);
    }

    /// get leggings slot 7
    pub fn get_leggings(&self) -> Option<&ItemStack> {
        self.items.slots.get(7)
    }

    /// set boots slot 8
    pub fn set_boots(&mut self, item: ItemStack) {
        // todo check if item is boots
        self.items.set(8, item);
    }

    /// get boots slot 8
    pub fn get_boots(&self) -> Option<&ItemStack> {
        self.items.slots.get(8)
    }

    /// get Entity Equipment
    pub fn get_entity_equipment(&self) -> [EquipmentEntry; 6] {
        let mainhand = EquipmentEntry {
            slot: 0,
            item: self.get_main_hand().cloned().unwrap_or(ItemStack::EMPTY),
        };
        let offhand = EquipmentEntry {
            slot: 1,
            item: self.get_offhand().cloned().unwrap_or(ItemStack::EMPTY),
        };
        let boots = EquipmentEntry {
            slot: 2,
            item: self.get_boots().cloned().unwrap_or(ItemStack::EMPTY),
        };
        let leggings = EquipmentEntry {
            slot: 3,
            item: self.get_leggings().cloned().unwrap_or(ItemStack::EMPTY),
        };
        let chestplate = EquipmentEntry {
            slot: 4,
            item: self.get_chestplate().cloned().unwrap_or(ItemStack::EMPTY),
        };
        let helmet = EquipmentEntry {
            slot: 5,
            item: self.get_helmet().cloned().unwrap_or(ItemStack::EMPTY),
        };

        [mainhand, offhand, boots, leggings, chestplate, helmet]
    }
}
