use std::{any::TypeId, cell::SyncUnsafeCell, ptr::NonNull, sync::atomic::AtomicUsize};

use bumpalo::Bump;
use flecs_ecs::{core::World, macros::Component};
use fxhash::FxHashMap;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};

use crate::{event::event_queue::raw::TypedBumpPtr, thread_local::ThreadLocal};

mod raw;

#[derive(Component, Default)]
pub struct EventQueue {
    pub count: AtomicUsize,
    inner: ThreadLocal<SyncUnsafeCell<heapless::Vec<TypedBumpPtr, 32>>>,
}

impl EventQueue {
    pub fn len(&mut self) -> usize {
        self.inner
            .iter_mut()
            .map(|inner| inner.get_mut().len())
            .sum()
    }

    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
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

        if inner.push(ptr).is_err() {
            return Ok(());
            // bail!("Event queue is full");
        }

        assert!(!inner.is_empty());

        self.count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }

    pub fn reset(&mut self) {
        self.inner
            .iter_mut()
            .for_each(|inner| inner.get_mut().clear());

        self.count = AtomicUsize::new(0);
    }
}

#[derive(Component)]
pub struct ThreadLocalBump {
    // todo: ThreadLocal has initialize logic which can slow down things if we are using it frequently.
    // we probably want to pre-initialize these on all threads that will need access.
    locals: Box<[Bump]>,
}

impl ThreadLocalBump {
    #[must_use]
    pub fn new(world: &World) -> Self {
        let num_locals = world.get_stage_count();
        Self {
            locals: (0..num_locals).map(|_| Bump::new()).collect(),
        }
    }

    #[must_use]
    pub fn get(&self, world: &World) -> &Bump {
        let id = world.stage_id();
        let id = usize::try_from(id).expect("failed to convert stage id");
        &self.locals[id]
    }

    pub fn par_iter_mut(&mut self) -> impl ParallelIterator<Item = &mut Bump> {
        self.locals.par_iter_mut()
    }
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
