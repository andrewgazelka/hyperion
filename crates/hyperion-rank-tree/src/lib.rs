use clap::ValueEnum;

pub mod inventory;
pub mod item;
pub mod skin;

mod util;

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
#[repr(C)]
pub enum Rank {
    /// ![Widget Example](https://i.imgur.com/pW7v0Xn.png)
    ///
    /// The stick is the starting rank.
    Stick, // -> [Pickaxe | Sword | Bow ]

    Archer,
    Sword,
    Miner,

    Excavator,

    Mage,
    Knight,
    Builder,
}

pub enum Team {
    Red,
    White,
    Blue,
}
