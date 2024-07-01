use std::ops::{Deref, DerefMut};

use flecs_ecs::core::World;

const NUM_THREADS: usize = 8;

/// Thread-local in flecs environment
#[derive(Debug, Default)]
pub struct ThreadLocal<T> {
    locals: [T; NUM_THREADS],
}

impl<T> Deref for ThreadLocal<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.locals
    }
}

impl<T> DerefMut for ThreadLocal<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.locals
    }
}

impl<T: Default> ThreadLocal<T> {
    #[must_use]
    pub fn new_defaults() -> Self {
        let locals = core::array::from_fn(|_| T::default());
        Self { locals }
    }
}

impl<T> ThreadLocal<T> {
    pub fn new_with<F>(f: F) -> Self
    where
        F: Fn(usize) -> T,
    {
        Self {
            locals: core::array::from_fn(f),
        }
    }

    #[must_use]
    pub fn get(&self, world: &World) -> &T {
        let id = world.stage_id();
        let id = usize::try_from(id).expect("failed to convert stage id");
        &self.locals[id]
    }
}
