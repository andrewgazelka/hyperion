use flecs_ecs::{
    core::{Entity, World},
    macros::Component,
};
use valence_protocol::Hand;

use crate::simulation::handlers::PacketSwitchQuery;

type EventFn<T> = fn(&mut PacketSwitchQuery<'_>, &T);

#[derive(Component, Default)]
pub struct GlobalEventHandlers {
    pub click: EventHandlers<Hand>,
}

pub struct EventHandlers<T> {
    handlers: Vec<EventFn<T>>,
}

pub struct EventHandler<T> {
    handler: EventFn<T>,
}

impl<T> EventHandler<T> {
    pub fn new(handler: EventFn<T>) -> Self {
        Self { handler }
    }

    pub fn trigger(&self, world: &mut PacketSwitchQuery<'_>, event: &T) {
        (self.handler)(world, event);
    }
}

impl<T> Default for EventHandlers<T> {
    fn default() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }
}

impl<T> EventHandlers<T> {
    pub fn trigger_all(&self, world: &mut PacketSwitchQuery<'_>, event: &T) {
        for handler in &self.handlers {
            handler(world, event);
        }
    }

    pub fn register(&mut self, handler: EventFn<T>) {
        self.handlers.push(handler);
    }
}

pub struct PlayerJoinServer {
    pub username: String,
    pub entity: Entity,
}
