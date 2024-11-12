use hyperion_inventory::PlayerInventory;
use valence_protocol::ItemKind;

use crate::{
    util::{AttackDamage, ItemBuilder},
    Rank, Team,
};

impl Team {
    pub const fn build_item(self) -> ItemBuilder {
        let kind = match self {
            Self::Red => ItemKind::RedTerracotta,
            Self::White => ItemKind::WhiteTerracotta,
            Self::Blue => ItemKind::BlueTerracotta,
        };

        ItemBuilder::new(kind)
    }
}

impl Rank {
    pub fn apply_inventory(self, team: Team, inventory: &mut PlayerInventory) {
        const PICKAXE_SLOT: u16 = 1;
        const BUILD_SLOT: u16 = 2;
        const ARROW_SLOT: u16 = 7;
        const GUI_SLOT: u16 = 8;

        let default_pickaxe = ItemBuilder::new(ItemKind::WoodenPickaxe).build();
        inventory.set_hotbar(PICKAXE_SLOT, default_pickaxe);

        let default_build_item = team.build_item().count(16).build();
        inventory.set_hotbar(BUILD_SLOT, default_build_item);

        match self {
            Self::Stick => {
                let stick = ItemBuilder::new(ItemKind::Stick)
                    .glowing()
                    .name("§dG§5R§dO§5N§dK§5-§dTHUNK")
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(0, stick);
            }

            Self::Archer => {
                let bow = ItemBuilder::new(ItemKind::Bow).build();

                inventory.set_hotbar(0, bow);

                let arrow = ItemBuilder::new(ItemKind::Arrow).count(64).build();

                inventory.set_hotbar(ARROW_SLOT, arrow);
            }
            Self::Sword => {
                let sword = ItemBuilder::new(ItemKind::StoneSword)
                    .name("§3VERTEX§f-§bSLICER")
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(0, sword);
            }

            Self::Miner => {
                let pickaxe = ItemBuilder::new(ItemKind::WoodenPickaxe)
                    .add_attribute(AttackDamage(2.0))
                    .build();

                inventory.set_hotbar(0, pickaxe);
            }

            Self::Mage => {
                let wand = ItemBuilder::new(ItemKind::WoodenShovel)
                    .glowing()
                    .add_attribute(AttackDamage(2.0))
                    .build();

                inventory.set_hotbar(0, wand);
            }
            Self::Knight => {
                let knight_sword = ItemBuilder::new(ItemKind::IronSword)
                    .name("§7§k-§r§7BESKAR-§8BLADE")
                    .glowing()
                    .add_attribute(AttackDamage(4.0))
                    .build();

                inventory.set_hotbar(0, knight_sword);
            }
            Self::Builder => {
                let builder_tool = ItemBuilder::new(ItemKind::GoldenPickaxe)
                    .add_attribute(AttackDamage(5.0))
                    .build();

                inventory.set_hotbar(0, builder_tool);
            }
        }

        let upgrade_item = ItemBuilder::new(ItemKind::FireworkStar)
            .name("Upgrades")
            .build();

        inventory.set_hotbar(GUI_SLOT, upgrade_item);
    }
}
