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
        const MAIN_SLOT: u16 = 0;
        const PICKAXE_SLOT: u16 = 1;
        const BUILD_SLOT: u16 = 2;

        const UPGRADE_SPEED_SLOT: u16 = 3;
        const UPGRADE_VISION_SLOT: u16 = 4;
        const UPGRADE_HEALTH_SLOT: u16 = 5;
        const UPGRADE_ARMOR_SLOT: u16 = 6;
        const UPGRADE_DAMAGE_SLOT: u16 = 7;

        const GUI_SLOT: u16 = 8;

        let upgrade_not_available = ItemBuilder::new(ItemKind::GrayDye);

        let upgrades = ["Speed", "Vision", "Health", "Armor", "Damage"];

        for (i, upgrade) in upgrades.into_iter().enumerate() {
            let slot = i as u16 + UPGRADE_SPEED_SLOT;
            inventory.set_hotbar(slot, upgrade_not_available.clone().name(upgrade).build());
        }

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

                inventory.set_hotbar(MAIN_SLOT, stick);
            }

            Self::Archer => {
                let bow = ItemBuilder::new(ItemKind::Bow).build();

                inventory.set_hotbar(MAIN_SLOT, bow);

                let arrow = ItemBuilder::new(ItemKind::Arrow).count(64).build();

                inventory.set(9, arrow).unwrap();
            }
            Self::Sword => {
                let sword = ItemBuilder::new(ItemKind::StoneSword)
                    .name("§3VERTEX§f-§bSLICER")
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(MAIN_SLOT, sword);
            }

            Self::Miner => {
                let torch = ItemBuilder::new(ItemKind::Torch)
                    .name("§3§lTORCH")
                    .glowing()
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(MAIN_SLOT, torch);

                let pickaxe = ItemBuilder::new(ItemKind::StonePickaxe)
                    .add_attribute(AttackDamage(2.0))
                    .build();

                inventory.set_hotbar(PICKAXE_SLOT, pickaxe);
            }

            Self::Mage => {
                let wand = ItemBuilder::new(ItemKind::WoodenShovel)
                    .glowing()
                    .add_attribute(AttackDamage(2.0))
                    .build();

                inventory.set_hotbar(MAIN_SLOT, wand);
            }
            Self::Knight => {
                let knight_sword = ItemBuilder::new(ItemKind::IronSword)
                    .name("§7§k-§r§7BESKAR-§8BLADE")
                    .glowing()
                    .add_attribute(AttackDamage(4.0))
                    .build();

                inventory.set_hotbar(MAIN_SLOT, knight_sword);
            }
            Self::Builder => {
                let builder_tool = ItemBuilder::new(ItemKind::GoldenPickaxe)
                    .add_attribute(AttackDamage(5.0))
                    .build();

                inventory.set_hotbar(MAIN_SLOT, builder_tool);
            }
            Rank::Excavator => {
                let pickaxe = ItemBuilder::new(ItemKind::IronPickaxe).build();

                inventory.set_hotbar(PICKAXE_SLOT, pickaxe);

                let minecart = ItemBuilder::new(ItemKind::Minecart)
                    .name("§3§lMINECART")
                    .glowing()
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(MAIN_SLOT, minecart);
            }
        }

        let upgrade_item = ItemBuilder::new(ItemKind::FireworkStar)
            .name("Upgrades")
            .build();

        inventory.set_hotbar(GUI_SLOT, upgrade_item);
    }
}
