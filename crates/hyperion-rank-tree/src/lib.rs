use clap::ValueEnum;

pub mod inventory;

mod util;

#[derive(Copy, Clone, Debug, ValueEnum)]
#[repr(C)]
pub enum Rank {
    /// ![Widget Example](https://i.imgur.com/pW7v0Xn.png)
    ///
    /// The stick is the starting rank.
    Stick, // -> [Pickaxe | Sword | Bow ]

    Bow,
    Sword,
    Pickaxe,

    Magician,
    Knight,
    Builder,
}
