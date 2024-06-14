use derive_more::{Deref, DerefMut};
use flecs_ecs::macros::Component;
use tokio::runtime::Runtime;

#[derive(Component, Deref, DerefMut)]
pub struct Tasks {
    runtime: Runtime,
}

impl Default for Tasks {
    fn default() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self { runtime }
    }
}
