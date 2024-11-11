use std::mem::offset_of;
use hyperion_inventory::PlayerInventory;
use valence_protocol::{nbt, nbt::Value, ItemKind, ItemStack};

mod inventory;
mod util;

#[derive(Copy, Clone, Debug)]
#[repr(C)]
enum Rank {
    /// ![Widget Example](https://i.imgur.com/pW7v0Xn.png)
    ///
    /// The stick is the starting rank.
    Stick,

    Pickaxe,
}

impl Rank {
}
