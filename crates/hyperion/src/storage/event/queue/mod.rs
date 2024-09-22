use std::marker::PhantomData;

use flecs_ecs::{
    core::{flecs, ComponentId, ComponentType, DataComponent, Struct, World, WorldGet},
    macros::Component,
};

use crate::simulation::event;

pub mod event_queue;
pub mod raw;

pub use event_queue::EventQueue;

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

pub trait Event: ReducedLifetime + sealed::Sealed + Send + Sync + 'static {
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
    event::DestroyBlock => destroy_block,
    event::PluginMessage<'static> => plugin_message
}

pub trait ReducedLifetime {
    type Reduced<'a>
    where
        Self: 'a;

    fn reduce<'a>(self) -> Self::Reduced<'a>;
}

macro_rules! simple_reduce {
    ($($event:ty),+) => {
        $(

        impl ReducedLifetime for $event {
            type Reduced<'a>

            = Self where Self: 'a;

            fn reduce<'a>(self) -> Self::Reduced<'a> {
                self
            }
        }

    )+
    }
}

simple_reduce!(
    event::ItemDropEvent,
    event::SwingArm,
    event::AttackEntity,
    event::Command,
    event::PostureUpdate,
    event::DestroyBlock
);

impl ReducedLifetime for event::PluginMessage<'static> {
    type Reduced<'a>
    = event::PluginMessage<'a> where Self: 'a;

    fn reduce<'a>(self) -> Self::Reduced<'a> {
        self
    }
}
