use clap::ValueEnum;

pub mod inventory;
pub mod skin;

mod util;

#[derive(Copy, Clone, Debug, ValueEnum)]
#[repr(C)]
pub enum Rank {
    /// ![Widget Example](https://i.imgur.com/pW7v0Xn.png)
    ///
    /// The stick is the starting rank.
    Stick, // -> [Pickaxe | Sword | Bow ]

    Archer,
    Sword,
    Miner,

    Mage,
    Knight,
    Builder,
}

pub enum Team {
    Red,
    White,
    Blue,
}
