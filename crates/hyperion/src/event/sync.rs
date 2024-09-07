use flecs_ecs::macros::Component;

#[derive(Component, Default)]
pub struct GlobalEventHandlers {
    pub set_username: EventHandlers<SetUsernameEvent>,
}

pub struct EventHandlers<T> {
    handlers: Vec<fn(&mut T)>,
}

impl<T> Default for EventHandlers<T> {
    fn default() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }
}

impl<T> EventHandlers<T> {
    pub fn trigger_all(&self, event: &mut T) {
        for handler in &self.handlers {
            handler(event);
        }
    }

    pub fn register(&mut self, handler: fn(&mut T)) {
        self.handlers.push(handler);
    }
}

pub struct SetUsernameEvent {
    pub username: String,
}
