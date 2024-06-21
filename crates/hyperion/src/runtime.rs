//! See [`AsyncRuntime`].

use derive_more::{Deref, DerefMut};
use flecs_ecs::macros::Component;

/// Wrapper around [`tokio::runtime::Runtime`]
#[derive(Component, Deref, DerefMut)]
pub struct AsyncRuntime {
    runtime: tokio::runtime::Runtime,
}

impl Default for AsyncRuntime {
    fn default() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self { runtime }
    }
}
