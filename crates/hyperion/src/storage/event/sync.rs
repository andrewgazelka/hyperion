use flecs_ecs::{
    core::{Entity, World},
    macros::Component,
};

type EventFn<T, O = ()> = dyn Fn(&World, &mut T) -> O + Send + Sync + 'static;

#[derive(Component, Default)]
pub struct GlobalEventHandlers {
    pub join_server: EventHandlers<PlayerJoinServer>,
}

pub struct EventHandlers<T> {
    handlers: Vec<Box<EventFn<T>>>,
}

impl<T> Default for EventHandlers<T> {
    fn default() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }
}

impl<T> EventHandlers<T> {
    pub fn trigger_all(&self, world: &World, event: &mut T) {
        for handler in &self.handlers {
            handler(world, event);
        }
    }

    pub fn register(&mut self, handler: impl Fn(&World, &mut T) + Send + Sync + 'static) {
        let handler = Box::new(handler);
        self.handlers.push(handler);
    }
}

pub struct PlayerJoinServer {
    pub username: String,
    pub entity: Entity,
}
