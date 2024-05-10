use std::{mem, ops::RangeInclusive};

use evenio::component::Component;
use itertools::{Either, Itertools};
use thiserror::Error;
use tracing::warn;
use valence_protocol::packets::play::{
    click_slot_c2s::SlotChange, entity_equipment_update_s2c::EquipmentEntry,
};
use valence_server::{ItemKind, ItemStack};

#[derive(Debug)]
pub struct Inventory<const T: usize> {
    pub slots: [ItemStack; T],
}

impl<const T: usize> Inventory<T> {
    /// Constructs a new inventory with the given size.
    const fn new() -> Self {
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
        mem::replace(&mut self.slots[index], ItemStack::EMPTY)
    }

    /// Get all items in the inventory
    /// to send to client
    #[must_use]
    pub const fn get_items(&self) -> &[ItemStack; T] {
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

#[derive(Debug, Error)]
pub enum SlotChangeError {
    #[error("More items than expected")]
    MoreItems,
    #[error("To much items in stack")]
    ToMuchInStack,
    #[error("Change index not found")]
    SlotNotFound,
    #[error("Less items than expected")]
    LessItems,
    #[error("Armor slot can only contain armor")]
    NoArmor,
}

/// Struct represents the result of [`append_slot_change`]
pub struct AppendSlotChange {
    /// flag if the equipment should be updated
    pub update_equipment: bool,
}

impl Default for PlayerInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerInventory {
    /// Constructs a new player inventory.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: Inventory::new(),
            main_hand: 36,
            carried_item: ItemStack::EMPTY,
        }
    }

    /// Append the client proposed slot change to the inventory
    /// It checks, that the player does not invent not existing items by summing up all slots before and after the change
    pub fn append_slot_change(
        &mut self,
        slot_change: &Vec<SlotChange>,
        client_proposed_cursor: &ItemStack,
        allow_less: bool,
    ) -> Result<AppendSlotChange, SlotChangeError> {
        // construct result struct
        let mut result = AppendSlotChange {
            update_equipment: false,
        };

        // Bitmap of the affected slots
        let mut slots: u128 = 0;
        // set the bitmap for changed slots
        for change in slot_change {
            if change.idx > 45 {
                warn!("Slot not found {:?}", change.idx);
                return Err(SlotChangeError::SlotNotFound);
            }
            slots |= 1 << change.idx;

            // check if the stack is not to big
            if change.stack.count < 0 || change.stack.count > change.stack.item.max_stack() {
                warn!("To much items in stack {:?}", change.stack);
                // no negative items
                return Err(SlotChangeError::ToMuchInStack);
            }

            // check if armor slot
            let armor_slot_ok = match change.idx {
                5 => Self::is_helmet(&change.stack),
                6 => Self::is_chestplate(&change.stack),
                7 => Self::is_leggings(&change.stack),
                8 => Self::is_boots(&change.stack),
                _ => true,
            };
            if !armor_slot_ok {
                warn!("Armor slot can only contain armor {:?}", change.stack);
                return Err(SlotChangeError::NoArmor);
            }

            // check if the visible items are updated
            if matches!(change.idx, 5..=8 | 45) || change.idx as usize == self.main_hand {
                result.update_equipment = true;
            }
        }

        // sum up all items of a kind
        // slot_change.iter().map(|change| &change.stack.item)
        let count_per_item_kind = slot_change
            .iter()
            // ignore air
            .filter(|change| change.stack.item != ItemKind::Air)
            .map(|change| (change.stack.item, change.stack.count as isize))
            .into_grouping_map()
            .sum();

        for (item, count) in count_per_item_kind {
            // sum up all items of a kind
            let mut current_count = self
                .items
                .slots
                .iter()
                .enumerate()
                .filter(|(idx, stack)| stack.item == item && slots & (1 << idx) > 0)
                .map(|(_, stack)| stack.count as isize)
                .sum::<isize>();
            let mut proposed_count = count;

            // check cursor slots
            if self.carried_item.item == item {
                warn!("Carried {:?}", self.carried_item);
                current_count += self.carried_item.count as isize;
            }
            if client_proposed_cursor.item == item {
                warn!("Proposed {:?}", client_proposed_cursor);
                proposed_count += client_proposed_cursor.count as isize;
            }

            // check if the player does not invent items
            if proposed_count > current_count {
                warn!(
                    "More items than expected {:?} p:{proposed_count} c:{current_count}",
                    item
                );
                return Err(SlotChangeError::MoreItems);
            }
            // check if the player does not destroy items
            if !allow_less && proposed_count < current_count {
                warn!(
                    "Less items than expected {:?}p:{proposed_count} c:{current_count}",
                    item
                );
                return Err(SlotChangeError::MoreItems);
            }
        }

        // all checks passed
        // apply changes
        for change in slot_change {
            self.items.set(change.idx as usize, change.stack.clone());
            self.carried_item = client_proposed_cursor.clone();
        }

        Ok(result)
    }

    /// Get the Carried Item
    #[must_use]
    pub const fn get_carried_item(&self) -> &ItemStack {
        &self.carried_item
    }

    /// Set item at first available spot
    pub fn set_first_available(&mut self, item: ItemStack) -> Either<(), ItemStack> {
        // try hotbar
        let item = match self.items.set_first_available(36..=44, item) {
            Either::Left(()) => return Either::Left(()),
            Either::Right(item) => item,
        };

        // try inventory
        let item = match self.items.set_first_available(9..=35, item) {
            Either::Left(()) => return Either::Left(()),
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
    #[must_use]
    pub fn get_offhand(&self) -> Option<&ItemStack> {
        self.items.slots.get(45)
    }

    /// get item in the main hand
    #[must_use]
    pub fn get_main_hand(&self) -> Option<&ItemStack> {
        self.items.slots.get(self.main_hand)
    }

    /// set main hand index to
    pub fn set_main_hand(&mut self, index: usize) -> Result<(), ()> {
        if !(36..=44).contains(&index) {
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
    #[must_use]
    pub fn get_helmet(&self) -> Option<&ItemStack> {
        self.items.slots.get(5)
    }

    /// set chestplate slot 6
    pub fn set_chestplate(&mut self, item: ItemStack) {
        // todo check if item is chestplate
        self.items.set(6, item);
    }

    /// get chestplate slot 6
    #[must_use]
    pub fn get_chestplate(&self) -> Option<&ItemStack> {
        self.items.slots.get(6)
    }

    /// set leggings slot 7
    pub fn set_leggings(&mut self, item: ItemStack) {
        // todo check if item is leggings
        self.items.set(7, item);
    }

    /// get leggings slot 7
    #[must_use]
    pub fn get_leggings(&self) -> Option<&ItemStack> {
        self.items.slots.get(7)
    }

    /// set boots slot 8
    pub fn set_boots(&mut self, item: ItemStack) {
        // todo check if item is boots
        self.items.set(8, item);
    }

    /// get boots slot 8
    #[must_use]
    pub fn get_boots(&self) -> Option<&ItemStack> {
        self.items.slots.get(8)
    }

    /// get Entity Equipment
    #[must_use]
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

    /// check if the item is a helmet
    #[must_use]
    pub fn is_helmet(check_item: &ItemStack) -> bool {
        matches!(
            check_item.item,
            ItemKind::LeatherHelmet
                | ItemKind::ChainmailHelmet
                | ItemKind::IronHelmet
                | ItemKind::GoldenHelmet
                | ItemKind::DiamondHelmet
                | ItemKind::NetheriteHelmet
                | ItemKind::TurtleHelmet
                | ItemKind::Air
        ) || (check_item.item.eq(&ItemKind::Pumpkin) && check_item.count == 1)
    }

    /// check if the item is a chestplate
    #[must_use]
    pub const fn is_chestplate(check_item: &ItemStack) -> bool {
        matches!(
            check_item.item,
            ItemKind::LeatherChestplate
                | ItemKind::ChainmailChestplate
                | ItemKind::IronChestplate
                | ItemKind::GoldenChestplate
                | ItemKind::DiamondChestplate
                | ItemKind::NetheriteChestplate
                | ItemKind::Elytra
                | ItemKind::Air
        )
    }

    /// check if the item is a leggings
    #[must_use]
    pub const fn is_leggings(check_item: &ItemStack) -> bool {
        matches!(
            check_item.item,
            ItemKind::LeatherLeggings
                | ItemKind::ChainmailLeggings
                | ItemKind::IronLeggings
                | ItemKind::GoldenLeggings
                | ItemKind::DiamondLeggings
                | ItemKind::NetheriteLeggings
                | ItemKind::Air
        )
    }

    /// check if the item is a boots
    #[must_use]
    pub const fn is_boots(check_item: &ItemStack) -> bool {
        matches!(
            check_item.item,
            ItemKind::LeatherBoots
                | ItemKind::ChainmailBoots
                | ItemKind::IronBoots
                | ItemKind::GoldenBoots
                | ItemKind::DiamondBoots
                | ItemKind::NetheriteBoots
                | ItemKind::Air
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_inventory() {
        let mut inventory = Inventory::<46>::new();
        let item = ItemStack::new(ItemKind::AcaciaBoat, 1, None);
        inventory.set(0, item.clone());
        assert_eq!(inventory.slots[0], item);
    }

    // test append_slot_change
    #[test]
    fn test_append_slot_change() {
        let mut inventory = PlayerInventory::new();
        let item = ItemStack::new(ItemKind::AcaciaBoat, 1, None);
        let slot_change = vec![SlotChange {
            idx: 0,
            stack: item.clone(),
        }];
        inventory
            .append_slot_change(&slot_change, &ItemStack::EMPTY, false)
            .unwrap();
        assert_eq!(inventory.items.slots[0], item);
    }

    // append_slot_change with more items
    #[test]
    fn test_append_slot_change_more_items() {}

    // prepare basic inventory for tests
    fn prepare_inventory() -> PlayerInventory {
        let mut inventory = PlayerInventory::new();
        inventory
            .items
            .set(36, ItemStack::new(ItemKind::AcaciaBoat, 1, None));

        inventory
    }
}
