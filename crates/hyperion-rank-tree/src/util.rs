use valence_protocol::{nbt, nbt::Value, ItemKind, ItemStack};

/// A builder for creating Minecraft items with NBT data
#[must_use]
pub struct ItemBuilder {
    kind: ItemKind,
    count: i8,
    nbt: Option<nbt::Compound<String>>,
}

/// Represents a Minecraft attribute that can be applied to items
pub trait Attribute: Copy {
    fn create_modifier(&self) -> nbt::Compound<String>;
}

// Example attribute implementations - now as value-carrying structs
#[derive(Copy, Clone, Debug)]
pub struct AttackDamage(pub f64);

#[derive(Copy, Clone, Debug)]
pub struct AttackSpeed(pub f64);

#[derive(Copy, Clone, Debug)]
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
        modifier.insert("AttributeName", "minecraft:generic.max_health".to_string());
        modifier.insert("Name", "generic.max_health".to_string());
        modifier.insert("Amount", self.0);
        modifier.insert("Operation", 0_i32);
        modifier
    }
}

impl ItemBuilder {
    pub const fn new(kind: ItemKind) -> Self {
        Self {
            kind,
            count: 1,
            nbt: None,
        }
    }

    /// Sets a custom name for the item
    pub fn name(mut self, name: impl Into<String>) -> Self {
        let nbt = self.nbt.get_or_insert_with(nbt::Compound::new);

        // Create or get existing display compound
        let display = match nbt.remove("display") {
            Some(Value::Compound(display)) => display,
            _ => nbt::Compound::new(),
        };

        // Create a new display compound with the name
        let mut new_display = display;
        new_display.insert("Name", Value::String(name.into()));

        // Insert the updated display compound
        nbt.insert("display", Value::Compound(new_display));
        self
    }

    pub const fn count(mut self, count: i8) -> Self {
        self.count = count;
        self
    }

    pub fn glowing(mut self) -> Self {
        let nbt = self.nbt.get_or_insert_with(nbt::Compound::new);
        nbt.insert(
            "Enchantments",
            Value::List(nbt::list::List::Compound(vec![nbt::Compound::new()])),
        );
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
        let _sword = ItemBuilder::new(ItemKind::DiamondSword)
            .count(1)
            .glowing()
            .add_attribute(AttackDamage(7.0))
            .add_attribute(AttackSpeed(1.6))
            .build();

        // Add assertions here
    }
}
