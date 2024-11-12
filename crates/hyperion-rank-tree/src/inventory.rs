use hyperion_inventory::PlayerInventory;
use valence_protocol::ItemKind;

use crate::{
    util::{AttackDamage, ItemBuilder},
    Rank,
};

impl Rank {
    #[must_use]
    pub fn inventory(self) -> PlayerInventory {
        let mut inventory = PlayerInventory::default();

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

                inventory.set_hotbar(7, arrow);
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

        inventory.set_hotbar(8, upgrade_item);

        inventory
    }
}
