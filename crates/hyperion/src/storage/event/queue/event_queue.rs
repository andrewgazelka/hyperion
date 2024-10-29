use derive_more::{Deref, DerefMut};
use flecs_ecs::macros::Component;

use crate::storage::{Event, ThreadLocalVec};

#[derive(Component, Deref, DerefMut)]
pub struct EventQueue<T>
where
    T: Event,
{
    // todo: maybe change to SOA vec
    inner: ThreadLocalVec<T>,
}

impl<T> Default for EventQueue<T>
where
    T: Event,
{
    fn default() -> Self {
        Self {
            inner: ThreadLocalVec::default(),
        }
    }
}

impl<T: Event> EventQueue<T> {
    pub fn drain(&mut self) -> impl Iterator<Item = T::Reduced<'_>> {
        self.inner.drain().map(super::ReducedLifetime::reduce)
    }

    // todo: think we need to reduce here too probably
    pub fn peek(&mut self) -> impl Iterator<Item = &mut T> {
        self.inner.iter_mut()
    }
}
