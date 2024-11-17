use std::sync::LazyLock;

use hyperion::simulation::skin::PlayerSkin;

use crate::{Rank, Team};

macro_rules! define_skins {
    ($($rank:ident => $file:literal),* $(,)?) => {
        $(
            define_team_skins!($rank => $file);
        )*
    };
}

macro_rules! define_team_skins {
    ($rank:ident => $file:literal) => {
        paste::paste! {
            static [<RED_ $rank>]: LazyLock<PlayerSkin> = LazyLock::new(|| {
                let skin = include_str!(concat!("skin/red/", $file, ".toml"));
                toml::from_str(skin).unwrap()
            });
            static [<BLUE_ $rank>]: LazyLock<PlayerSkin> = LazyLock::new(|| {
                let skin = include_str!(concat!("skin/blue/", $file, ".toml"));
                toml::from_str(skin).unwrap()
            });
            static [<GREEN_ $rank>]: LazyLock<PlayerSkin> = LazyLock::new(|| {
                let skin = include_str!(concat!("skin/green/", $file, ".toml"));
                toml::from_str(skin).unwrap()
            });
            static [<YELLOW_ $rank>]: LazyLock<PlayerSkin> = LazyLock::new(|| {
                let skin = include_str!(concat!("skin/yellow/", $file, ".toml"));
                toml::from_str(skin).unwrap()
            });
        }
    };
}

define_skins! {
    // STICK_SKIN => "stick",
    // SWORDSMAN_SKIN => "swordsman",
    KNIGHT_SKIN => "knight",
    // ARCHER_SKIN => "archer",
    // MAGE_SKIN => "mage",
    // BUILDER_SKIN => "builder",
    // MINER_SKIN => "miner",
    // EXCAVATOR_SKIN => "excavator"
}

impl Rank {
    #[must_use]
    pub fn skin(&self, team: Team) -> &'static PlayerSkin {
        match (team, self) {
            // (Team::Red, Self::Stick) => &RED_STICK_SKIN,
            // (Team::Red, Self::Sword) => &RED_SWORDSMAN_SKIN,
            // (Team::Red, Self::Archer) => &RED_ARCHER_SKIN,
            // (Team::Red, Self::Mage) => &RED_MAGE_SKIN,
            // (Team::Red, Self::Builder) => &RED_BUILDER_SKIN,
            // (Team::Red, Self::Miner) => &RED_MINER_SKIN,
            // (Team::Red, Self::Excavator) => &RED_EXCAVATOR_SKIN,

            // (Team::Blue, Self::Stick) => &BLUE_STICK_SKIN,
            // (Team::Blue, Self::Sword) => &BLUE_SWORDSMAN_SKIN,
            (Team::Blue, Self::Knight) => &BLUE_KNIGHT_SKIN,
            // (Team::Blue, Self::Archer) => &BLUE_ARCHER_SKIN,
            // (Team::Blue, Self::Mage) => &BLUE_MAGE_SKIN,
            // (Team::Blue, Self::Builder) => &BLUE_BUILDER_SKIN,
            // (Team::Blue, Self::Miner) => &BLUE_MINER_SKIN,
            // (Team::Blue, Self::Excavator) => &BLUE_EXCAVATOR_SKIN,

            // (Team::Green, Self::Stick) => &GREEN_STICK_SKIN,
            // (Team::Green, Self::Sword) => &GREEN_SWORDSMAN_SKIN,
            (Team::Green, Self::Knight) => &GREEN_KNIGHT_SKIN,
            // (Team::Green, Self::Archer) => &GREEN_ARCHER_SKIN,
            // (Team::Green, Self::Mage) => &GREEN_MAGE_SKIN,
            // (Team::Green, Self::Builder) => &GREEN_BUILDER_SKIN,
            // (Team::Green, Self::Miner) => &GREEN_MINER_SKIN,
            // (Team::Green, Self::Excavator) => &GREEN_EXCAVATOR_SKIN,

            // (Team::Yellow, Self::Stick) => &YELLOW_STICK_SKIN,
            // (Team::Yellow, Self::Sword) => &YELLOW_SWORDSMAN_SKIN,
            (Team::Yellow, Self::Knight) => &YELLOW_KNIGHT_SKIN,
            // (Team::Yellow, Self::Archer) => &YELLOW_ARCHER_SKIN,
            // (Team::Yellow, Self::Mage) => &YELLOW_MAGE_SKIN,
            // (Team::Yellow, Self::Builder) => &YELLOW_BUILDER_SKIN,
            // (Team::Yellow, Self::Miner) => &YELLOW_MINER_SKIN,
            // (Team::Yellow, Self::Excavator) => &YELLOW_EXCAVATOR_SKIN,
            _ => &RED_KNIGHT_SKIN, // Default fallback
        }
    }
}
