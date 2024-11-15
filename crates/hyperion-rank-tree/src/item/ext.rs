use flecs_ecs::core::Entity;
use valence_protocol::{nbt::Value, ItemStack};

mod private {
    pub trait Sealed {}
}

trait ItemExt: private::Sealed {
    fn handler(&self) -> Option<Entity>;
}

impl private::Sealed for ItemStack {}

impl ItemExt for ItemStack {
    fn handler(&self) -> Option<Entity> {
        let nbt = self.nbt.as_ref()?;
        let handler = nbt.get("Handler")?;

        let Value::Long(id) = handler else {
            return None;
        };

        let id = bytemuck::cast(*id);
        Some(Entity(id))
    }
}
