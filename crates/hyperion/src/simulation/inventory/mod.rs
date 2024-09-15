use flecs_ecs::macros::Component;
use tracing::info;
use valence_protocol::ItemStack;

pub mod action;
pub mod parser;

pub type PlayerInventory = Inventory<46>;

/// Placeholder; this will be added later.
#[derive(Component, Debug)]
pub struct Inventory<const T: usize = 46> {
    slots: [ItemStack; T],
    hand_slot: u16,
}

impl<const T: usize> Default for Inventory<T> {
    fn default() -> Self {
        Self {
            slots: [ItemStack::EMPTY; T],
            hand_slot: 0,
        }
    }
}

impl<const T: usize> Inventory<T> {
    pub fn set(&mut self, index: u16, stack: ItemStack) {
        if let Some(item) = self.get_mut(index) {
            *item = stack;
        }
    }

    pub fn set_cursor(&mut self, index: u16) {
        self.hand_slot = index;
    }

    #[must_use]
    pub fn get_held(&self) -> Option<&ItemStack> {
        info!("get_held is {}", self.hand_slot);
        self.get_hand_slot(self.hand_slot)
    }

    #[must_use]
    pub fn get(&self, index: u16) -> Option<&ItemStack> {
        self.slots.get(usize::from(index))
    }

    pub fn get_mut(&mut self, index: u16) -> Option<&mut ItemStack> {
        self.slots.get_mut(usize::from(index))
    }

    pub fn set_slot(&mut self, index: u16, stack: ItemStack) {
        if let Some(item) = self.get_mut(index) {
            *item = stack;
        }
    }

    pub fn swap(&mut self, index_a: u16, index_b: u16) {
        let index_a = usize::from(index_a);
        let index_b = usize::from(index_b);

        self.slots.swap(index_a, index_b);
    }

    #[must_use]
    pub fn get_hand_slot(&self, idx: u16) -> Option<&ItemStack> {
        const HAND_START_SLOT: u16 = 36;
        const HAND_END_SLOT: u16 = 45;

        let idx = idx + HAND_START_SLOT;

        if idx >= HAND_END_SLOT {
            return None;
        }

        self.get(idx)
    }

    pub fn get_hand_slot_mut(&mut self, idx: u16) -> Option<&mut ItemStack> {
        const HAND_START_SLOT: u16 = 36;
        const HAND_END_SLOT: u16 = 45;

        let idx = idx + HAND_START_SLOT;

        if idx >= HAND_END_SLOT {
            return None;
        }

        self.get_mut(idx)
    }

    pub fn set_hand_slot(&mut self, idx: u16, stack: ItemStack) {
        const HAND_START_SLOT: u16 = 36;
        const HAND_END_SLOT: u16 = 45;

        let idx = idx + HAND_START_SLOT;

        if idx >= HAND_END_SLOT {
            return;
        }

        self.set(idx, stack);
    }
}

#[must_use]
pub fn slot_index_from_hand(hand_idx: u8) -> u16 {
    const HAND_START_SLOT: u16 = 36;
    const HAND_END_SLOT: u16 = 45;

    let hand_idx = u16::from(hand_idx);
    let hand_idx = hand_idx + HAND_START_SLOT;

    if hand_idx >= HAND_END_SLOT {
        return 0;
    }

    hand_idx
}

// todo: not sure if this is correct
pub const OFFHAND_SLOT: u16 = 45;
