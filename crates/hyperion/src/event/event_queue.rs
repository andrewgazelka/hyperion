use std::{any::TypeId, ptr::NonNull};

use anyhow::bail;
use bumpalo::Bump;
use derive_more::{Deref, DerefMut};
use flecs_ecs::macros::Component;
use thread_local::ThreadLocal;

use crate::event::event_queue::raw::{BumpPtr, RawQueue};

mod raw;

#[derive(Component)]
pub struct EventQueue {
    /// we want to be able to append to this `EventQueue` from any thread so we can do things concurrently.
    /// For instance, if we are iterating over player A who has a packet to send to player B, we want to be able to
    /// append to Player B's queue from Player A's thread.
    ///
    /// We are not using a `crossbeam_queue::ArrayQueue` because it requrires consuming the queue to iterate over it.
    inner: RawQueue,
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl EventQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RawQueue::new(1024),
        }
    }

    pub fn push<T: 'static>(&self, elem: T, allocator: &Allocator) -> anyhow::Result<()> {
        let bump = allocator.locals.get_or_default();

        let id = TypeId::of::<T>();
        let ptr: &mut T = bump.alloc(elem);

        let ptr = NonNull::from(ptr);
        let ptr = ptr.cast();

        let ptr = BumpPtr::new(id, ptr);

        if self.inner.push(ptr).is_err() {
            bail!("Event queue is full");
        }

        Ok(())
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

#[derive(Default, Component, Deref, DerefMut)]
pub struct Allocator {
    // todo: ThreadLocal has initialize logic which can slow down things if we are using it frequently.
    // we proably want to pre-initialize these on all threads that will need access.
    locals: ThreadLocal<Bump>,
}

#[derive(Default)]
pub struct EventQueueIterator<'a> {
    registered_ids: heapless::Vec<TypeId, 16>,
    /// [`TypeId`]s are sufficiently hashed
    registered: heapless::Vec<Box<dyn FnMut(*mut ()) + 'a>, 16>,
}

impl<'a> EventQueueIterator<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            registered_ids: heapless::Vec::default(),
            registered: heapless::Vec::new(),
        }
    }

    pub fn register<T: 'static>(&mut self, mut f: impl FnMut(&mut T) + 'a) -> anyhow::Result<()> {
        let id = TypeId::of::<T>();
        if self.registered_ids.push(id).is_err() {
            bail!("map is full");
        }

        let f = move |x: *mut ()| {
            let x = unsafe { &mut *x.cast::<T>() };
            f(x);
        };

        // box
        let f = Box::new(f);

        let _ = self.registered.push(f);
        Ok(())
    }

    fn get_fn(&mut self, id: TypeId) -> Option<&mut dyn FnMut(*mut ())> {
        let position = self.registered_ids.iter().position(|x| *x == id)?;
        let f = self.registered.get_mut(position).unwrap();
        let f = &mut **f;
        Some(f)
    }

    pub fn run(&mut self, queue: &mut EventQueue) {
        for ptr in queue.inner.iter_mut() {
            let Some(f) = self.get_fn(ptr.id()) else {
                continue;
            };

            f(ptr.elem().as_ptr());
        }
    }
}

#[cfg(test)]
mod tests {
    use rayon::iter::{IntoParallelIterator, ParallelIterator};

    use super::*;

    #[test]
    fn test_event_queue() {
        let mut queue = EventQueue::new();
        let allocator = Allocator::default();

        (0..8).into_par_iter().for_each(|i| {
            queue.push(i, &allocator).unwrap();
        });

        queue.push("hello", &allocator).unwrap();

        let mut iter = EventQueueIterator::default();

        iter.register::<i32>(|x| {
            println!("{x}");
        })
        .unwrap();

        iter.run(&mut queue);
    }
}
