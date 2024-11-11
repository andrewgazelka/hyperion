use valence_protocol::{nbt, ItemKind, ItemStack};
use valence_protocol::nbt::Value;
use hyperion_inventory::PlayerInventory;
use crate::Rank;
use crate::util::{AttackDamage, ItemBuilder};

impl Rank {
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
            Self::Pickaxe => {
                let pickaxe = ItemBuilder::new(ItemKind::WoodenPickaxe)
                    .add_attribute(AttackDamage(2.0))
                    .build();
                
                inventory.set_hotbar(0, pickaxe);
            }
        }

        inventory
    }
}