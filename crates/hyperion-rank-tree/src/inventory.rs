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
        let mut pickaxe_slot: u16 = 1;

        if self == Rank::Miner {
            pickaxe_slot = 1;
        }

        let main_slot: u16 = 0;
        let build_slot: u16 = 2;
        let arrow_slot: u16 = 7;
        let gui_slot: u16 = 8;

        let default_pickaxe = ItemBuilder::new(ItemKind::WoodenPickaxe).build();
        inventory.set_hotbar(pickaxe_slot, default_pickaxe);

        let default_build_item = team.build_item().count(16).build();
        inventory.set_hotbar(build_slot, default_build_item);

        match self {
            Self::Stick => {
                let stick = ItemBuilder::new(ItemKind::Stick)
                    .glowing()
                    .name("§dG§5R§dO§5N§dK§5-§dTHUNK")
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(main_slot, stick);
            }

            Self::Archer => {
                let bow = ItemBuilder::new(ItemKind::Bow).build();

                inventory.set_hotbar(main_slot, bow);

                let arrow = ItemBuilder::new(ItemKind::Arrow).count(64).build();

                inventory.set_hotbar(arrow_slot, arrow);
            }
            Self::Sword => {
                let sword = ItemBuilder::new(ItemKind::StoneSword)
                    .name("§3VERTEX§f-§bSLICER")
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(main_slot, sword);
            }

            Self::Miner => {
                let torch = ItemBuilder::new(ItemKind::Torch)
                    .name("§3§lTORCH")
                    .glowing()
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(main_slot, torch);

                let pickaxe = ItemBuilder::new(ItemKind::StonePickaxe)
                    .add_attribute(AttackDamage(2.0))
                    .build();

                inventory.set_hotbar(pickaxe_slot, pickaxe);
            }

            Self::Mage => {
                let wand = ItemBuilder::new(ItemKind::WoodenShovel)
                    .glowing()
                    .add_attribute(AttackDamage(2.0))
                    .build();

                inventory.set_hotbar(main_slot, wand);
            }
            Self::Knight => {
                let knight_sword = ItemBuilder::new(ItemKind::IronSword)
                    .name("§7§k-§r§7BESKAR-§8BLADE")
                    .glowing()
                    .add_attribute(AttackDamage(4.0))
                    .build();

                inventory.set_hotbar(main_slot, knight_sword);
            }
            Self::Builder => {
                let builder_tool = ItemBuilder::new(ItemKind::GoldenPickaxe)
                    .add_attribute(AttackDamage(5.0))
                    .build();

                inventory.set_hotbar(main_slot, builder_tool);
            }
            Rank::Excavator => {
                let pickaxe = ItemBuilder::new(ItemKind::IronPickaxe).build();

                inventory.set_hotbar(pickaxe_slot, pickaxe);

                let minecart = ItemBuilder::new(ItemKind::Minecart)
                    .name("§3§lMINECART")
                    .glowing()
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(main_slot, minecart);
            }
        }

        let upgrade_item = ItemBuilder::new(ItemKind::FireworkStar)
            .name("Upgrades")
            .build();

        inventory.set_hotbar(gui_slot, upgrade_item);
    }
}
