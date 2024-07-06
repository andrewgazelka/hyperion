use std::{any::TypeId, cell::SyncUnsafeCell, ptr::NonNull};

use bumpalo::Bump;
use derive_more::{Deref, DerefMut};
use flecs_ecs::{core::World, macros::Component};
use fxhash::FxHashMap;

use crate::{
    component::blocks::chunk::ThreadLocalVec, event::event_queue::raw::TypedBumpPtr,
    thread_local::ThreadLocal,
};

pub mod raw;

#[derive(Component, Default)]
pub struct EventQueue {
    inner: ThreadLocalVec<TypedBumpPtr>,
}

impl EventQueue {
    pub fn len(&mut self) -> usize {
        self.inner
            .iter_mut()
            .map(|inner| inner.get_mut().len())
            .sum()
    }

    pub fn is_empty(&mut self) -> bool {
        self.inner
            .iter_mut()
            .map(SyncUnsafeCell::get_mut)
            .all(|x| x.is_empty())
    }

    pub fn push<T: 'static>(
        &self,
        elem: T,
        allocator: &ThreadLocalBump,
        world: &World,
    ) -> anyhow::Result<()> {
        let bump = allocator.get(world);

        let id = TypeId::of::<T>();
        let ptr: &mut T = bump.alloc(elem);

        let ptr = NonNull::from(ptr);
        let ptr = ptr.cast();

        let ptr = TypedBumpPtr::new(id, ptr);

        let inner = self.inner.get(world);
        let inner = unsafe { &mut *inner.get() };

        inner.push(ptr);

        assert!(!inner.is_empty());

        Ok(())
    }

    pub fn reset(&mut self) {
        self.inner
            .iter_mut()
            .for_each(|inner| inner.get_mut().clear());
    }
}

#[derive(Component, Deref, DerefMut, Default)]
pub struct ThreadLocalBump {
    // todo: ThreadLocal has initialize logic which can slow down things if we are using it frequently.
    // we probably want to pre-initialize these on all threads that will need access.
    locals: ThreadLocal<Bump>,
}

// todo: improve code.
/// 🚨 There is basically a 0% chance this is safe and not slightly UB depending on how it is used.
#[derive(Default)]
pub struct EventQueueIterator<'a> {
    registered: FxHashMap<TypeId, Box<dyn FnMut(*mut ()) + 'a>>,
}

impl<'a> EventQueueIterator<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            registered: FxHashMap::default(),
        }
    }

    pub fn register<E: 'static>(&mut self, mut f: impl FnMut(&mut E) + 'a) {
        let id = TypeId::of::<E>();

        // todo: is this safe
        let g = move |ptr: *mut ()| unsafe { f(&mut *ptr.cast::<E>()) };

        let previous = self.registered.insert(id, Box::new(g));

        assert!(
            previous.is_none(),
            "there is already a handler for this type"
        );
    }

    pub fn run(&mut self, queue: &mut EventQueue) {
        queue
            .inner
            .iter_mut()
            .map(|inner| unsafe { &mut *inner.get() })
            .flat_map(|inner| inner.iter_mut())
            .for_each(|ptr| {
                let Some(res) = self.registered.get_mut(&ptr.id()) else {
                    return;
                };

                let f = &mut **res;

                f(ptr.elem().as_ptr());
            });
    }
}

#[cfg(test)]
mod tests {
    // #[test]
    // fn test_event_queue() {
    //     let queue = EventQueue::new();
    //     let allocator = ThreadLocalBump::default();
    //
    //     (0..8).into_par_iter().for_each(|i| {
    //         queue.push(i, &allocator).unwrap();
    //     });
    //
    //     queue.push("hello", &allocator).unwrap();
    //
    //     let mut iter = EventQueueIterator::default();
    //
    //     iter.register::<i32>(|x, ()| {
    //         println!("{x}");
    //     });
    //
    //     iter.run(&queue, &mut ());
    // }
}
