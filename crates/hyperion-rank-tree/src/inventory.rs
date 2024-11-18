use flecs_ecs::core::{World, WorldGet};
use hyperion_inventory::PlayerInventory;
use hyperion_item::builder::{AttackDamage, Color, ItemBuilder};
use valence_protocol::ItemKind;

use crate::{Handles, Rank, Team};

impl Team {
    pub const fn build_item(self) -> ItemBuilder {
        let kind = match self {
            Self::Blue => ItemKind::BlueTerracotta,
            Self::Green => ItemKind::GreenTerracotta,
            Self::Red => ItemKind::RedTerracotta,
            Self::Yellow => ItemKind::YellowTerracotta,
        };

        ItemBuilder::new(kind)
    }
}

impl Rank {
    pub fn apply_inventory(self, team: Team, inventory: &mut PlayerInventory, world: &World) {
        const MAIN_SLOT: u16 = 0;
        const PICKAXE_SLOT: u16 = 1;
        const BUILD_SLOT: u16 = 2;

        const UPGRADE_START_SLOT: u16 = 3;

        const GUI_SLOT: u16 = 8;

        let upgrade_not_available = ItemBuilder::new(ItemKind::GrayDye);

        inventory.clear();

        let color = match team {
            Team::Red => Color(255, 0, 0),
            Team::Blue => Color(0, 0, 255),
            Team::Green => Color(0, 255, 0),
            Team::Yellow => Color(255, 255, 0),
        };

        let boots = ItemBuilder::new(ItemKind::LeatherBoots)
            .color(color)
            .build();

        inventory.set_boots(boots);

        let upgrades = ["Speed", "Vision", "Health", "Armor", "Damage"];

        world.get::<&Handles>(|handles| {
            for (i, upgrade) in upgrades.into_iter().enumerate() {
                let slot = u16::try_from(i).unwrap() + UPGRADE_START_SLOT;
                let item = upgrade_not_available
                    .clone()
                    .name(upgrade)
                    .handler(handles.speed)
                    .build();
                inventory.set_hotbar(slot, item);
            }
        });

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
            Self::Excavator => {
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
