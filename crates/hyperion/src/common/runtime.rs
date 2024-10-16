//! See [`AsyncRuntime`].

use std::sync::Arc;

use derive_more::{Deref, DerefMut};
use flecs_ecs::macros::Component;

/// Wrapper around [`tokio::runtime::Runtime`]
#[derive(Component, Deref, DerefMut, Clone)]
pub struct AsyncRuntime {
    runtime: Arc<tokio::runtime::Runtime>,
}

impl Default for AsyncRuntime {
    fn default() -> Self {
        #[expect(
            clippy::unwrap_used,
            reason = "this is very unlikely to fail and even if it does it will be in \
                      initialization"
        )]
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let runtime = Arc::new(runtime);

        Self { runtime }
    }
}
