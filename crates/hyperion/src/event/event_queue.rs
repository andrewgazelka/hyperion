use std::{any::TypeId, marker::PhantomData, ptr::NonNull};

use anyhow::bail;
use bumpalo::Bump;
use flecs_ecs::{core::World, macros::Component};
use fxhash::FxHashMap;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};

use crate::event::event_queue::raw::{RawQueue, TypedBumpPtr};

mod raw;

#[derive(Component)]
pub struct EventQueue {
    /// we want to be able to append to this `EventQueue` from any thread so we can do things concurrently.
    /// For instance, if we are iterating over player A who has a packet to send to player B, we want to be able to
    /// append to Player B's queue from Player A's thread.
    ///
    /// We are not using a `crossbeam_queue::ArrayQueue` because it requires consuming the queue to iterate over it.
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

        if self.inner.push(ptr).is_err() {
            bail!("Event queue is full");
        }

        Ok(())
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

#[derive(Component)]
pub struct ThreadLocalBump {
    // todo: ThreadLocal has initialize logic which can slow down things if we are using it frequently.
    // we proably want to pre-initialize these on all threads that will need access.
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
pub struct EventQueueIterator<T> {
    registered: FxHashMap<TypeId, fn(*mut (), *mut ())>,
    _phantom: PhantomData<T>,
}

impl<T> Default for EventQueueIterator<T> {
    fn default() -> Self {
        Self {
            registered: FxHashMap::default(),
            _phantom: PhantomData,
        }
    }
}

impl<T> EventQueueIterator<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            registered: FxHashMap::default(),
            _phantom: PhantomData,
        }
    }

    pub fn register<E: 'static>(&mut self, f: fn(&mut E, &mut T)) {
        let id = TypeId::of::<E>();

        // todo: is this safe
        let g = unsafe { core::mem::transmute::<fn(&mut E, &mut T), fn(*mut (), *mut ())>(f) };

        let previous = self.registered.insert(id, g);

        assert!(
            previous.is_none(),
            "there is already a handler for this type"
        );
    }

    fn get_fn(&self, id: TypeId) -> Option<fn(*mut (), &mut T)> {
        let res = self.registered.get(&id)?;
        let res = *res;
        let res = unsafe { core::mem::transmute::<fn(*mut (), *mut ()), fn(*mut (), &mut T)>(res) };
        Some(res)
    }

    pub fn run(&self, queue: &EventQueue, t: &mut T) {
        for ptr in queue.inner.iter() {
            let Some(f) = self.get_fn(ptr.id()) else {
                continue;
            };

            f(ptr.elem().as_ptr(), t);
        }
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
