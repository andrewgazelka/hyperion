use std::marker::PhantomData;

use derive_more::{Deref, DerefMut};
use flecs_ecs::{
    core::{flecs, ComponentId, ComponentType, DataComponent, Struct, World, WorldGet},
    macros::Component,
};

use crate::{component::blocks::chunk::ThreadLocalVec, event};

pub mod raw;

#[derive(Component, Deref, DerefMut)]
pub struct EventQueue<T>
where
    T: Send + Sync + 'static,
{
    // todo: maybe change to SOA vec
    inner: ThreadLocalVec<T>,
}

impl<T> Default for EventQueue<T>
where
    T: Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            inner: ThreadLocalVec::default(),
        }
    }
}

impl Events {
    pub fn push<E: Event>(&self, event: E, world: &World) {
        E::input(event, self, world);
    }
}

struct SendSyncPtr<T>(*const T, PhantomData<T>);

unsafe impl<T> Send for SendSyncPtr<T> {}
unsafe impl<T> Sync for SendSyncPtr<T> {}

mod sealed {
    pub trait Sealed {}
}

pub trait Event: sealed::Sealed {
    fn input(elem: Self, events: &Events, world: &World);
}

fn register_and_pointer<T: ComponentId + DataComponent + ComponentType<Struct>>(
    world: &World,
    elem: T,
) -> *const T {
    world.component::<T>().add_trait::<flecs::Sparse>();

    world.set(elem);

    world.get::<&T>(|x: &T| std::ptr::from_ref::<T>(x))
}

macro_rules! define_events {
    ($($event:ty => $queue:ident),+ $(,)?) => {
        #[derive(Component)]
        pub struct Events {
            $(
                $queue: SendSyncPtr<EventQueue<$event>>,
            )+
        }

        impl Events {
            #[must_use] pub fn initialize(world: &World) -> Self {
                Self {
                    $(
                        $queue: SendSyncPtr(register_and_pointer(world, EventQueue::<$event>::default()), PhantomData),
                    )+
                }
            }
        }

        $(
            impl Event for $event {
                fn input(elem: Self, events: &Events, world: &World) {
                    unsafe {
                        (*events.$queue.0).push(elem, world);
                    }
                }
            }

            impl sealed::Sealed for $event {}
        )+
    };
}

// create the Events struct
define_events! {
    event::ItemDropEvent => item_drop,
    event::SwingArm => swing_arm,
    event::AttackEntity => attack,
    event::Command => command,
    event::PostureUpdate => posture_update,
}
