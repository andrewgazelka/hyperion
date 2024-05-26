use std::{mem, ops::RangeInclusive};

use anyhow::ensure;
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
    main_hand: u16,
    /// carried item
    ///
    /// This item will be none when player closes inventory
    carried_item: ItemStack,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SlotChangeError {
    #[error("More items than expected in inventory")]
    MoreItemsThanExpected,
    #[error("Fewer items than expected in inventory")]
    FewerItemsThanExpected,
    #[error("Invalid stack count")]
    InvalidStackCount,
    #[error("Change index not found")]
    SlotNotFound,
    #[error("Armor slot can only contain armor")]
    NonArmorInArmorSlot,
}

/// Struct represents the result of [`PlayerInventory::try_append_changes`]
#[derive(Debug, Eq, PartialEq)]
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
    /// It checks, that the player does not invent not-existing items by summing up all slots before and after the change
    pub fn try_append_changes(
        &mut self,
        slot_change: &[SlotChange],
        client_proposed_cursor: &ItemStack,
        allow_fewer_than_expected: bool,
    ) -> Result<AppendSlotChange, SlotChangeError> {
        const SLOT_COUNT: i16 = 46;

        // construct result struct
        let mut result = AppendSlotChange {
            update_equipment: false,
        };

        // check if the cursor stack is not too big
        if client_proposed_cursor.count < 0
            || client_proposed_cursor.count > client_proposed_cursor.item.max_stack()
        {
            return Err(SlotChangeError::InvalidStackCount);
        }

        // Bitmap of the affected slots
        let mut slots: u128 = 0;
        // set the bitmap for changed slots
        for change in slot_change {
            if change.idx >= SLOT_COUNT {
                warn!("Slot not found {:?}", change.idx);
                return Err(SlotChangeError::SlotNotFound);
            }
            slots |= 1 << change.idx;

            // check if the stack is not to big
            if change.stack.count < 0 || change.stack.count > change.stack.item.max_stack() {
                return Err(SlotChangeError::InvalidStackCount);
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
                return Err(SlotChangeError::NonArmorInArmorSlot);
            }

            // check if the visible items are updated
            if matches!(change.idx, 5..=8 | 45)
                || u16::try_from(change.idx).unwrap() == self.main_hand
            {
                result.update_equipment = true;
            }
        }

        // sum up all items of a kind
        // slot_change.iter().map(|change| &change.stack.item)
        let mut count_per_item_kind = slot_change
            .iter()
            // ignore air
            .filter(|change| change.stack.item != ItemKind::Air)
            .map(|change| (change.stack.item, change.stack.count as isize))
            .into_grouping_map()
            .sum();

        if client_proposed_cursor.item != ItemKind::Air {
            // add the cursor item to the count
            count_per_item_kind
                .entry(client_proposed_cursor.item)
                .and_modify(|count| *count += client_proposed_cursor.count as isize)
                .or_insert(client_proposed_cursor.count as isize);
        }

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
            let proposed_count = count;

            // check cursor slots
            if self.carried_item.item == item {
                warn!("Carried {:?}", self.carried_item);
                current_count += self.carried_item.count as isize;
            }

            // check if the player does not invent items
            if proposed_count > current_count {
                warn!(
                    "More items than expected {:?} proposed:{proposed_count} \
                     current:{current_count}",
                    item
                );
                return Err(SlotChangeError::MoreItemsThanExpected);
            }
            // check if the player does not destroy items
            if !allow_fewer_than_expected && proposed_count < current_count {
                warn!(
                    "Fewer items than expected {:?}p:{proposed_count} c:{current_count}",
                    item
                );
                return Err(SlotChangeError::FewerItemsThanExpected);
            }
        }

        // all checks passed
        // apply changes
        for change in slot_change {
            self.items
                .set(usize::try_from(change.idx).unwrap(), change.stack.clone());
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
        self.items.slots.get(self.main_hand as usize)
    }

    /// get item in the main hand mutable
    #[must_use]
    pub fn get_main_hand_mut(&mut self) -> Option<&mut ItemStack> {
        self.items.slots.get_mut(self.main_hand as usize)
    }

    /// set main hand index to
    pub fn set_main_hand(&mut self, index: u16) -> anyhow::Result<()> {
        ensure!(
            (36..=44).contains(&index),
            "main hand can only be in the hotbar"
        );
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
        prepare_tracing();
        let mut inventory = Inventory::<46>::new();
        let item = ItemStack::new(ItemKind::AcaciaBoat, 1, None);
        inventory.set(0, item.clone());
        assert_eq!(inventory.slots[0], item);
    }

    // test append_slot_change
    #[test]
    fn test_move_1_boat() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![
            SlotChange {
                idx: 11,
                stack: ItemStack::new(ItemKind::AcaciaBoat, 1, None),
            },
            SlotChange {
                idx: 36,
                stack: ItemStack::EMPTY,
            },
        ];
        // Move the boat from slot 36 to slot 11
        let result = inventory
            .try_append_changes(&slot_change, &ItemStack::EMPTY, false)
            .unwrap();
        assert!(result.update_equipment);
        assert_eq!(
            inventory.items.slots[11],
            ItemStack::new(ItemKind::AcaciaBoat, 1, None)
        );
        assert_eq!(inventory.items.slots[36], ItemStack::EMPTY);
    }

    // test move 42 items of mossy cobblestone
    #[test]
    fn test_move_42_mossy_cobblestone() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![
            SlotChange {
                idx: 11,
                stack: ItemStack::new(ItemKind::MossyCobblestone, 42, None),
            },
            SlotChange {
                idx: 20,
                stack: ItemStack::EMPTY,
            },
        ];
        // Move the mossy cobblestone from slot 36 to slot 11
        let result = inventory
            .try_append_changes(&slot_change, &ItemStack::EMPTY, false)
            .unwrap();
        assert!(!result.update_equipment);
        assert_eq!(
            inventory.items.slots[11],
            ItemStack::new(ItemKind::MossyCobblestone, 42, None)
        );
        assert_eq!(inventory.items.slots[20], ItemStack::EMPTY);
    }

    // split stack of 64 golden apples
    #[test]
    fn test_split_stack_64_golden_apples() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![
            SlotChange {
                idx: 11,
                stack: ItemStack::new(ItemKind::GoldenApple, 32, None),
            },
            SlotChange {
                idx: 20,
                stack: ItemStack::new(ItemKind::GoldenApple, 32, None),
            },
            SlotChange {
                idx: 38,
                stack: ItemStack::EMPTY,
            },
        ];
        let result = inventory
            .try_append_changes(&slot_change, &ItemStack::EMPTY, false)
            .unwrap();
        assert!(!result.update_equipment);
        assert_eq!(
            inventory.items.slots[11],
            ItemStack::new(ItemKind::GoldenApple, 32, None)
        );
        assert_eq!(
            inventory.items.slots[20],
            ItemStack::new(ItemKind::GoldenApple, 32, None)
        );
        assert_eq!(inventory.items.slots[38], ItemStack::EMPTY);
    }

    // split to many items
    #[test]
    fn test_split_to_many_items() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![
            SlotChange {
                idx: 11,
                stack: ItemStack::new(ItemKind::GoldenApple, 32, None),
            },
            SlotChange {
                idx: 20,
                stack: ItemStack::new(ItemKind::GoldenApple, 33, None),
            },
            SlotChange {
                idx: 38,
                stack: ItemStack::EMPTY,
            },
        ];
        let result = inventory.try_append_changes(&slot_change, &ItemStack::EMPTY, false);
        assert_eq!(result, Err(SlotChangeError::MoreItemsThanExpected));
    }

    // pick up cursor
    #[test]
    fn test_pick_up_cursor() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![SlotChange {
            idx: 20,
            stack: ItemStack::EMPTY,
        }];
        let result = inventory
            .try_append_changes(
                &slot_change,
                &ItemStack::new(ItemKind::MossyCobblestone, 42, None),
                false,
            )
            .unwrap();
        assert!(!result.update_equipment);
        assert_eq!(inventory.items.slots[20], ItemStack::EMPTY);
        assert_eq!(
            inventory.get_carried_item(),
            &ItemStack::new(ItemKind::MossyCobblestone, 42, None)
        );
    }

    // pick up to many items
    #[test]
    fn test_pick_up_to_many_items() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![SlotChange {
            idx: 20,
            stack: ItemStack::EMPTY,
        }];
        let result = inventory.try_append_changes(
            &slot_change,
            &ItemStack::new(ItemKind::MossyCobblestone, 43, None),
            false,
        );
        assert_eq!(result, Err(SlotChangeError::MoreItemsThanExpected));
    }

    // pick up half of the stack
    #[test]
    fn test_pick_up_half_of_stack() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![SlotChange {
            idx: 20,
            stack: ItemStack::new(ItemKind::MossyCobblestone, 21, None),
        }];
        let result = inventory
            .try_append_changes(
                &slot_change,
                &ItemStack::new(ItemKind::MossyCobblestone, 21, None),
                false,
            )
            .unwrap();
        assert!(!result.update_equipment);
        assert_eq!(
            inventory.items.slots[20],
            ItemStack::new(ItemKind::MossyCobblestone, 21, None)
        );
        assert_eq!(
            inventory.get_carried_item(),
            &ItemStack::new(ItemKind::MossyCobblestone, 21, None)
        );
    }

    // pick up to much but leave some
    #[test]
    fn test_pick_up_to_much_but_leave_some() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![SlotChange {
            idx: 20,
            stack: ItemStack::new(ItemKind::MossyCobblestone, 21, None),
        }];
        // Move the golden apples from slot 38 to slot 11 and 20
        let result = inventory.try_append_changes(
            &slot_change,
            &ItemStack::new(ItemKind::MossyCobblestone, 22, None),
            true,
        );
        assert_eq!(result, Err(SlotChangeError::MoreItemsThanExpected));
    }

    // pick up other item with cursor
    #[test]
    fn test_pick_up_other_item_with_cursor() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![SlotChange {
            idx: 20,
            stack: ItemStack::EMPTY,
        }];
        let result = inventory.try_append_changes(
            &slot_change,
            &ItemStack::new(ItemKind::Diamond, 64, None),
            false,
        );
        assert_eq!(result, Err(SlotChangeError::MoreItemsThanExpected));
    }

    // put cursor in slot
    #[test]
    fn test_put_cursor_in_slot() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::NetheriteIngot, 64, None);

        let slot_change = vec![SlotChange {
            idx: 9,
            stack: ItemStack::new(ItemKind::NetheriteIngot, 64, None),
        }];
        let result = inventory
            .try_append_changes(&slot_change, &ItemStack::EMPTY, false)
            .unwrap();
        assert!(!result.update_equipment);
        assert_eq!(
            inventory.items.slots[9],
            ItemStack::new(ItemKind::NetheriteIngot, 64, None)
        );
        assert_eq!(inventory.get_carried_item(), &ItemStack::EMPTY);
    }

    // put cursor in slot 44 and change equipment
    #[test]
    fn test_put_cursor_in_slot_44_and_change_equipment() {
        prepare_tracing();
        let mut inventory: PlayerInventory = prepare_inventory();
        inventory.set_main_hand(44).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::NetheriteIngot, 64, None);

        let slot_change = vec![SlotChange {
            idx: 44,
            stack: ItemStack::EMPTY,
        }];
        let result = inventory
            .try_append_changes(&slot_change, &ItemStack::EMPTY, false)
            .unwrap();
        assert!(result.update_equipment);
        assert_eq!(inventory.items.slots[44], ItemStack::EMPTY);
        assert_eq!(inventory.get_carried_item(), &ItemStack::EMPTY);
    }

    // put 1 item of cursor in slot
    #[test]
    fn test_put_1_item_of_cursor_in_slot() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::NetheriteIngot, 64, None);

        let slot_change = vec![SlotChange {
            idx: 9,
            stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
        }];
        let result = inventory
            .try_append_changes(
                &slot_change,
                &ItemStack::new(ItemKind::NetheriteIngot, 63, None),
                false,
            )
            .unwrap();
        assert!(!result.update_equipment);
        assert_eq!(
            inventory.items.slots[9],
            ItemStack::new(ItemKind::NetheriteIngot, 1, None)
        );
        assert_eq!(
            inventory.get_carried_item(),
            &ItemStack::new(ItemKind::NetheriteIngot, 63, None)
        );
    }

    // put 1 item of cursor in slot but leave it in cursor
    #[test]
    fn test_put_1_item_of_cursor_in_slot_but_leave_it_in_cursor() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::NetheriteIngot, 64, None);

        let slot_change = vec![SlotChange {
            idx: 9,
            stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
        }];
        let result = inventory.try_append_changes(
            &slot_change,
            &ItemStack::new(ItemKind::NetheriteIngot, 64, None),
            false,
        );

        assert_eq!(result, Err(SlotChangeError::MoreItemsThanExpected));
    }

    // split cursor with 4 items in multiple slots and leave some in cursor
    #[test]
    fn test_split_cursor_with_4_items_in_multiple_slots_and_leave_some_in_cursor() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::NetheriteIngot, 5, None);

        let slot_change = vec![
            SlotChange {
                idx: 9,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
            },
            SlotChange {
                idx: 10,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
            },
            SlotChange {
                idx: 11,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
            },
            SlotChange {
                idx: 12,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
            },
        ];
        let result = inventory
            .try_append_changes(
                &slot_change,
                &ItemStack::new(ItemKind::NetheriteIngot, 1, None),
                true,
            )
            .unwrap();
        assert!(!result.update_equipment);
        assert_eq!(
            inventory.items.slots[9],
            ItemStack::new(ItemKind::NetheriteIngot, 1, None)
        );
        assert_eq!(
            inventory.items.slots[10],
            ItemStack::new(ItemKind::NetheriteIngot, 1, None)
        );
        assert_eq!(
            inventory.items.slots[11],
            ItemStack::new(ItemKind::NetheriteIngot, 1, None)
        );
        assert_eq!(
            inventory.items.slots[12],
            ItemStack::new(ItemKind::NetheriteIngot, 1, None)
        );
        assert_eq!(
            inventory.get_carried_item(),
            &ItemStack::new(ItemKind::NetheriteIngot, 1, None)
        );
    }

    // split cursor with 4 items in multiple slots and leave to much in cursor
    #[test]
    fn test_split_cursor_with_4_items_in_multiple_slots_and_leave_to_much_in_cursor() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::NetheriteIngot, 5, None);

        let slot_change = vec![
            SlotChange {
                idx: 9,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
            },
            SlotChange {
                idx: 10,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
            },
            SlotChange {
                idx: 11,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
            },
            SlotChange {
                idx: 12,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 1, None),
            },
        ];
        let result = inventory.try_append_changes(
            &slot_change,
            &ItemStack::new(ItemKind::NetheriteIngot, 2, None),
            false,
        );
        assert_eq!(result, Err(SlotChangeError::MoreItemsThanExpected));
    }

    // split cursor with 47 items in 3 slots and leave the rest in cursor
    #[test]
    fn test_split_cursor_with_47_items_in_3_slots_and_leave_the_rest_in_cursor() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::NetheriteIngot, 47, None);

        let slot_change = vec![
            SlotChange {
                idx: 9,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 15, None),
            },
            SlotChange {
                idx: 10,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 15, None),
            },
            SlotChange {
                idx: 11,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 15, None),
            },
        ];
        let result = inventory
            .try_append_changes(
                &slot_change,
                &ItemStack::new(ItemKind::NetheriteIngot, 2, None),
                true,
            )
            .unwrap();
        assert!(!result.update_equipment);
        assert_eq!(
            inventory.items.slots[9],
            ItemStack::new(ItemKind::NetheriteIngot, 15, None)
        );
        assert_eq!(
            inventory.items.slots[10],
            ItemStack::new(ItemKind::NetheriteIngot, 15, None)
        );
        assert_eq!(
            inventory.items.slots[11],
            ItemStack::new(ItemKind::NetheriteIngot, 15, None)
        );
        assert_eq!(
            inventory.get_carried_item(),
            &ItemStack::new(ItemKind::NetheriteIngot, 2, None)
        );
    }

    // put the right armor in the armor slots but with no armor there
    #[test]
    fn test_put_the_right_armor_in_the_armor_slots() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![
            SlotChange {
                idx: 5,
                stack: ItemStack::new(ItemKind::ChainmailHelmet, 1, None),
            },
            SlotChange {
                idx: 6,
                stack: ItemStack::new(ItemKind::ChainmailChestplate, 1, None),
            },
            SlotChange {
                idx: 7,
                stack: ItemStack::new(ItemKind::ChainmailLeggings, 1, None),
            },
            SlotChange {
                idx: 8,
                stack: ItemStack::new(ItemKind::ChainmailBoots, 1, None),
            },
        ];
        let result = inventory.try_append_changes(&slot_change, &ItemStack::EMPTY, false);
        assert_eq!(result, Err(SlotChangeError::MoreItemsThanExpected));
    }

    // put helmet to helmet
    #[test]
    fn test_put_helmet_to_helmet() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::ChainmailHelmet, 1, None);

        let slot_change = vec![SlotChange {
            idx: 5,
            stack: ItemStack::new(ItemKind::ChainmailHelmet, 1, None),
        }];
        let result = inventory
            .try_append_changes(&slot_change, &ItemStack::EMPTY, false)
            .unwrap();
        assert!(result.update_equipment);
        assert_eq!(
            inventory.items.slots[5],
            ItemStack::new(ItemKind::ChainmailHelmet, 1, None)
        );

        assert_eq!(inventory.get_carried_item(), &ItemStack::EMPTY);
    }

    // put helmet to chestplate
    #[test]
    fn test_put_helmet_to_chestplate() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::ChainmailHelmet, 1, None);

        let slot_change = vec![SlotChange {
            idx: 6,
            stack: ItemStack::new(ItemKind::ChainmailHelmet, 1, None),
        }];
        let result = inventory.try_append_changes(&slot_change, &ItemStack::EMPTY, false);
        assert_eq!(result, Err(SlotChangeError::NonArmorInArmorSlot));
    }

    // put to many items in cursor stack
    #[test]
    fn test_put_to_many_items_in_cursor_stack() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory
            .items
            .set(9, ItemStack::new(ItemKind::NetheriteIngot, 64, None));
        inventory
            .items
            .set(10, ItemStack::new(ItemKind::NetheriteIngot, 64, None));

        let slot_change = vec![
            SlotChange {
                idx: 9,
                stack: ItemStack::EMPTY,
            },
            SlotChange {
                idx: 10,
                stack: ItemStack::new(ItemKind::NetheriteIngot, 63, None),
            },
        ];
        let result = inventory.try_append_changes(
            &slot_change,
            &ItemStack::new(ItemKind::NetheriteIngot, 65, None),
            false,
        );
        assert_eq!(result, Err(SlotChangeError::InvalidStackCount));
    }

    // put to many items in slot
    // add Golden Apple to cursor and then put it in slot 38
    #[test]
    fn test_put_to_many_items_in_slot() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();
        inventory.carried_item = ItemStack::new(ItemKind::GoldenApple, 4, None);

        let slot_change = vec![SlotChange {
            idx: 38,
            stack: ItemStack::new(ItemKind::GoldenApple, 68, None),
        }];
        let result = inventory.try_append_changes(&slot_change, &ItemStack::EMPTY, false);
        assert_eq!(result, Err(SlotChangeError::InvalidStackCount));
    }

    // negative number in cursor
    #[test]
    fn test_negative_number_in_cursor() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![SlotChange {
            idx: 38,
            stack: ItemStack::EMPTY,
        }];
        let result = inventory.try_append_changes(
            &slot_change,
            &ItemStack::new(ItemKind::GoldenApple, -1, None),
            false,
        );
        assert_eq!(result, Err(SlotChangeError::InvalidStackCount));
    }

    // negative number in slot
    #[test]
    fn test_negative_number_in_slot() {
        prepare_tracing();
        let mut inventory = prepare_inventory();
        inventory.set_main_hand(36).unwrap();

        let slot_change = vec![SlotChange {
            idx: 38,
            stack: ItemStack::new(ItemKind::GoldenApple, -1, None),
        }];
        let result = inventory.try_append_changes(&slot_change, &ItemStack::EMPTY, false);
        assert_eq!(result, Err(SlotChangeError::InvalidStackCount));
    }

    // prepare basic inventory for tests
    fn prepare_inventory() -> PlayerInventory {
        let mut inventory = PlayerInventory::new();
        inventory
            .items
            .set(36, ItemStack::new(ItemKind::AcaciaBoat, 1, None));

        inventory
            .items
            .set(37, ItemStack::new(ItemKind::IronSword, 1, None));

        inventory
            .items
            .set(38, ItemStack::new(ItemKind::GoldenApple, 64, None));

        inventory
            .items
            .set(20, ItemStack::new(ItemKind::MossyCobblestone, 42, None));

        inventory
    }

    fn prepare_tracing() {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::DEBUG)
            .try_init();
    }
}
