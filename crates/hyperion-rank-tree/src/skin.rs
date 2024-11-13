use std::sync::LazyLock;

use hyperion::simulation::skin::PlayerSkin;

use crate::Rank;

macro_rules! define_skin {
    ($name:ident, $path:literal) => {
        static $name: LazyLock<PlayerSkin> = LazyLock::new(|| {
            let skin = include_str!($path);
            toml::from_str(skin).unwrap()
        });
    };
}

define_skin!(STICK_SKIN, "skin/stick.toml");
define_skin!(SWORDSMAN_SKIN, "skin/swordsman.toml");
define_skin!(KNIGHT_SKIN, "skin/knight.toml");
define_skin!(ARCHER_SKIN, "skin/archer.toml");
define_skin!(MAGE_SKIN, "skin/mage.toml");
define_skin!(BUILDER_SKIN, "skin/builder.toml");
define_skin!(MINER_SKIN, "skin/miner.toml");
define_skin!(EXCAVATOR_SKIN, "skin/excavator.toml");

impl Rank {
    #[must_use]
    pub fn skin(&self) -> &'static PlayerSkin {
        match self {
            Self::Stick => &STICK_SKIN,
            Self::Sword => &SWORDSMAN_SKIN,
            Self::Knight => &KNIGHT_SKIN,
            Self::Archer => &ARCHER_SKIN,
            Self::Mage => &MAGE_SKIN,
            Self::Builder => &BUILDER_SKIN,
            Self::Miner => &MINER_SKIN,
            Rank::Excavator => &EXCAVATOR_SKIN,
        }
    }
}
