use derive_more::{Constructor, Deref, DerefMut};
use flecs_ecs::{
    core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World, WorldGet},
    macros::Component,
    prelude::Module,
};
use hyperion::storage::{EventHandler, EventHandlers, GlobalEventHandlers};
use valence_protocol::{nbt, Hand};

pub mod builder;

#[derive(Component)]
pub struct ItemModule;

#[derive(Component, Constructor, Deref, DerefMut)]
pub struct Handler {
    on_click: EventHandler<Hand>,
}

impl Module for ItemModule {
    fn module(world: &World) {
        world.import::<hyperion_inventory::InventoryModule>();
        world.component::<Handler>();

        world.get::<&mut GlobalEventHandlers>(|handlers| {
            handlers.click.register(|query, hand| {
                let world = query.world;
                let inventory = &mut *query.inventory;

                let stack = inventory.get_cursor();

                if stack.is_empty() {
                    return;
                }

                let Some(nbt) = stack.nbt.as_ref() else {
                    return;
                };

                let Some(handler) = nbt.get("Handler") else {
                    return;
                };

                let nbt::Value::Long(id) = handler else {
                    return;
                };

                let id: u64 = bytemuck::cast(*id);

                let handler = world.entity_from_id(id);

                handler.try_get::<&Handler>(|handler| {
                    let on_interact = &handler.on_click;
                    on_interact.trigger(query, hand);
                });
            });
        });
    }
}
