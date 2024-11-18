use flecs_ecs::{core::Entity, macros::Component};
use valence_protocol::Hand;

use crate::simulation::handlers::PacketSwitchQuery;

pub type EventFn<T> = fn(&mut PacketSwitchQuery<'_>, &T);

pub struct CommandCompletionRequest<'a> {
    pub query: &'a str,
    pub id: i32,
}

#[derive(Component, Default)]
pub struct GlobalEventHandlers {
    pub click: EventHandlers<Hand>,

    // todo: this should be a lifetime for<'a>
    pub completion: EventHandlers<CommandCompletionRequest<'static>>,
}

pub struct EventHandlers<T> {
    handlers: Vec<EventFn<T>>,
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
