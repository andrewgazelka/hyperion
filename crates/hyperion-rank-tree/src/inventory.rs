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
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(0, stick);
            }

            Self::Bow => {
                let bow = ItemBuilder::new(ItemKind::Bow).build();

                inventory.set_hotbar(0, bow);
            }
            Self::Sword => {
                let sword = ItemBuilder::new(ItemKind::StoneSword)
                    .add_attribute(AttackDamage(3.0))
                    .build();

                inventory.set_hotbar(0, sword);
            }

            Self::Pickaxe => {
                let pickaxe = ItemBuilder::new(ItemKind::WoodenPickaxe)
                    .add_attribute(AttackDamage(2.0))
                    .build();

                inventory.set_hotbar(0, pickaxe);
            }

            Self::Magician => {
                let wand = ItemBuilder::new(ItemKind::WoodenShovel)
                    .glowing()
                    .add_attribute(AttackDamage(2.0))
                    .build();

                inventory.set_hotbar(0, wand);
            }
            Self::Knight => {
                let knight_sword = ItemBuilder::new(ItemKind::IronSword)
                    .add_attribute(AttackDamage(4.0))
                    .build();

                inventory.set_hotbar(0, knight_sword);
            }
            Self::Builder => {
                let builder_tool = ItemBuilder::new(ItemKind::GoldenPickaxe)
                    .add_attribute(AttackDamage(5.0))
                    .build();
            }
        }

        let upgrade_item = ItemBuilder::new(ItemKind::FireworkStar)
            .name("Upgrades")
            .build();

        inventory.set_hotbar(8, upgrade_item);

        inventory
    }
}
