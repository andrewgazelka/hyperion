//! See [Delta] for more information.

use derive_more::{Deref, DerefMut, From};
use evenio::prelude::Component;

/// Tracks changes in a value
#[derive(Component)]
pub struct Delta<T> {
    /// The previous value of the tracker.
    previous: T,
    /// The current value of the tracker.
    current: T,
}

impl<T: Clone> Delta<T> {
    /// Creates a new tracker with the given value.
    pub fn new(value: T) -> Self {
        Self {
            previous: value.clone(),
            current: value,
        }
    }

    /// Updates the tracker with the given function.
    pub fn update<F>(&mut self, f: F)
    where
        F: FnOnce(&mut T),
    {
        f(&mut self.current);
    }

    /// The previous value of the tracker; if there is only one value, this is the same as
    /// [`current`](Self::current).
    pub const fn previous(&self) -> &T {
        &self.previous
    }

    /// The current value of the tracker.
    pub const fn current(&self) -> &T {
        &self.current
    }

    /// Updates the previous value of the tracker with the current value.
    pub fn update_previous(&mut self) {
        self.previous = self.current.clone();
    }
}

/// Previous value
#[derive(Component, Deref, DerefMut, From)]
#[repr(transparent)]
pub struct Prev<T> {
    previous: T,
}
