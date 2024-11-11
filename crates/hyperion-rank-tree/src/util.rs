use valence_protocol::{nbt, ItemKind, ItemStack};
use valence_protocol::nbt::Value;

/// A builder for creating Minecraft items with NBT data
pub struct ItemBuilder {
    kind: ItemKind,
    count: i8,
    nbt: Option<nbt::Compound<String>>,
}

/// Represents a Minecraft attribute that can be applied to items
pub trait Attribute {
    fn create_modifier(&self) -> nbt::Compound<String>;
}

// Example attribute implementations - now as value-carrying structs
pub struct AttackDamage(pub f64);
pub struct AttackSpeed(pub f64);
pub struct MaxHealth(pub f64);

// Implement Attribute for each type
impl Attribute for AttackDamage {
    fn create_modifier(&self) -> nbt::Compound<String> {
        let mut modifier = nbt::Compound::new();
        modifier.insert(
            "AttributeName",
            "minecraft:generic.attack_damage".to_string(),
        );
        modifier.insert("Name", "generic.attack_damage".to_string());
        modifier.insert("Amount", self.0);
        modifier.insert("Operation", 0_i32);
        modifier
    }
}

impl Attribute for AttackSpeed {
    fn create_modifier(&self) -> nbt::Compound<String> {
        let mut modifier = nbt::Compound::new();
        modifier.insert(
            "AttributeName",
            "minecraft:generic.attack_speed".to_string(),
        );
        modifier.insert("Name", "generic.attack_speed".to_string());
        modifier.insert("Amount", self.0);
        modifier.insert("Operation", 0_i32);
        modifier
    }
}

impl Attribute for MaxHealth {
    fn create_modifier(&self) -> nbt::Compound<String> {
        let mut modifier = nbt::Compound::new();
        modifier.insert(
            "AttributeName",
            "minecraft:generic.max_health".to_string(),
        );
        modifier.insert("Name", "generic.max_health".to_string());
        modifier.insert("Amount", self.0);
        modifier.insert("Operation", 0_i32);
        modifier
    }
}

impl ItemBuilder {
    pub fn new(kind: ItemKind) -> Self {
        Self {
            kind,
            count: 1,
            nbt: None,
        }
    }

    pub fn count(mut self, count: i8) -> Self {
        self.count = count;
        self
    }

    pub fn glowing(mut self) -> Self {
        let nbt = self.nbt.get_or_insert_with(nbt::Compound::new);
        nbt.insert("Glowing", 1_i8);
        self
    }

    pub fn add_attribute(mut self, attribute: impl Attribute) -> Self {
        let nbt = self.nbt.get_or_insert_with(nbt::Compound::new);
        let mut modifiers = match nbt.remove("AttributeModifiers") {
            Some(Value::List(nbt::list::List::Compound(modifiers))) => modifiers,
            _ => Vec::new(),
        };

        modifiers.push(attribute.create_modifier());

        nbt.insert(
            "AttributeModifiers",
            Value::List(nbt::list::List::Compound(modifiers)),
        );
        self
    }

    pub fn build(self) -> ItemStack {
        ItemStack::new(self.kind, self.count, self.nbt)
    }
}

// Example usage
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_builder() {
        let sword = ItemBuilder::new(ItemKind::DiamondSword)
            .count(1)
            .glowing()
            .add_attribute(AttackDamage(7.0))
            .add_attribute(AttackSpeed(1.6))
            .build();

        // Add assertions here
    }
}