#![feature(allocator_api)]
#![feature(lint_reasons)]
#![feature(impl_trait_in_assoc_type)]
#![feature(thread_id_value)]
//! A simple thread-local storage abstraction for Rayon.

extern crate core;

use std::{
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(feature = "evenio")]
pub use evenio::*;
use rayon::iter::IntoParallelRefMutIterator;

#[derive(Debug)]
pub struct RayonRef<'a, S> {
    inner: &'a mut S,
}

#[expect(clippy::non_send_fields_in_send_ty, reason = "todo: is this safe?")]
unsafe impl<'a, S> Send for RayonRef<'a, S> {}
unsafe impl<'a, S> Sync for RayonRef<'a, S> {}

impl<'a, S> Deref for RayonRef<'a, S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'a, S> DerefMut for RayonRef<'a, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

/// A simple thread-local storage abstraction for Rayon.
#[cfg_attr(feature = "evenio", derive(::evenio::component::Component))]
#[derive(Debug)]
pub struct RayonLocal<S> {
    thread_locals: Box<[S]>,
    pinned: Box<[AtomicU64]>,
}

#[must_use]
pub fn count() -> usize {
    rayon::current_num_threads() + 1
}

impl<S> IntoIterator for RayonLocal<S> {
    type Item = S;

    type IntoIter = impl Iterator<Item = S>;

    fn into_iter(self) -> Self::IntoIter {
        self.thread_locals.into_vec().into_iter()
    }
}

impl<'a, S> IntoIterator for &'a mut RayonLocal<S> {
    type Item = &'a mut S;

    type IntoIter = impl Iterator<Item = &'a mut S>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[expect(clippy::non_send_fields_in_send_ty, reason = "todo: is this safe?")]
unsafe impl<S> Send for RayonLocal<S> {}
unsafe impl<S> Sync for RayonLocal<S> {}

impl<S: Default> Default for RayonLocal<S> {
    fn default() -> Self {
        Self::init_with_defaults()
    }
}

impl<S: Default> RayonLocal<S> {
    /// Create a new `RayonLocal` with the default value of `S` for each thread.
    #[must_use]
    pub fn init_with_defaults() -> Self {
        Self::init(S::default)
    }
}

impl<S> RayonLocal<S> {
    /// Create a new `RayonLocal` with the value of `S` provided by the closure for each thread.
    pub fn init(mut f: impl FnMut() -> S) -> Self {
        Self::init_with_index(|_| f())
    }

    pub fn init_with_index(f: impl FnMut(usize) -> S) -> Self {
        let num_threads = rayon::current_num_threads();

        let thread_locals = (0..=num_threads).map(f).collect();
        let pinned = (0..=num_threads).map(|_| AtomicU64::new(0)).collect();

        Self {
            thread_locals,
            pinned,
        }
    }

    #[must_use]
    pub fn par_iter_mut<'a>(&'a mut self) -> rayon::slice::IterMut<'a, S>
    where
        &'a mut [S]: IntoParallelRefMutIterator<'a>,
        S: Send,
    {
        self.get_all_mut().par_iter_mut()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut S> {
        self.get_all_mut().iter_mut()
    }

    #[must_use]
    pub fn as_refs(&self) -> RayonLocal<&S> {
        self.map_ref(|x| x)
    }

    pub fn map_ref<'a, T, F>(&'a self, f: F) -> RayonLocal<T>
    where
        F: Fn(&'a S) -> T,
    {
        let thread_locals = self.get_all().iter().map(f).collect();
        let pinned = (0..=rayon::current_num_threads())
            .map(|_| AtomicU64::new(0))
            .collect();

        RayonLocal {
            thread_locals,
            pinned,
        }
    }

    pub fn map<T, F>(self, f: F) -> RayonLocal<T>
    where
        F: Fn(S) -> T,
    {
        let thread_locals = self.thread_locals.into_vec();
        let thread_locals = thread_locals.into_iter().map(f).collect();

        let pinned = (0..=rayon::current_num_threads())
            .map(|_| AtomicU64::new(0))
            .collect();

        RayonLocal {
            thread_locals,
            pinned,
        }
    }

    #[must_use]
    pub fn idx(&self) -> usize {
        let len = self.thread_locals.len();
        let idx = rayon::current_thread_index().unwrap_or(len - 1);

        self.assert_pinned_thead(idx);

        idx
    }

    fn assert_pinned_thead(&self, idx: usize) {
        let atomic = self.pinned.get(idx).expect("thread idx is out of bounds");
        let current_thread_id = std::thread::current().id().as_u64().get();

        let previous = atomic.swap(current_thread_id, Ordering::Relaxed);

        assert!(
            !(previous != current_thread_id && previous != 0),
            "thread id changed from {previous} to {current_thread_id}"
        );
    }

    #[must_use]
    pub fn get_local(&self) -> &S {
        &self.thread_locals[self.idx()]
    }

    /// Get the thread local that is not bound to any `rayon` thread.
    /// This is often going to be what is going to be accessed if you're not in a `rayon` context.
    pub fn get_non_rayon_local(&mut self) -> &mut S {
        &mut self.thread_locals[self.idx()]
    }

    #[must_use]
    pub fn into_inner(self) -> Box<[S]> {
        self.thread_locals
    }

    /// Get a mutable reference to all thread-local values.
    pub fn get_all_mut(&mut self) -> &mut [S] {
        // todo: is there a more idiomatic way to do this?
        let start_ptr = self.thread_locals.as_mut_ptr();
        let len = self.thread_locals.len();
        unsafe { core::slice::from_raw_parts_mut(start_ptr.cast(), len) }
    }

    #[must_use]
    pub const fn get_all(&self) -> &[S] {
        // todo: is there a more idiomatic way to do this?
        let start_ptr = self.thread_locals.as_ptr();
        let len = self.thread_locals.len();
        unsafe { core::slice::from_raw_parts(start_ptr.cast(), len) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let local = RayonLocal::<i32>::init_with_defaults();
        assert_eq!(local.thread_locals.len(), rayon::current_num_threads() + 1);
        assert!(local.get_all().iter().all(|&x| x == 0));
    }

    // #[test]
    // fn test_get_all_locals() {
    //     let mut local = RayonLocal::<i32>::init();
    //     local
    //         .get_all_mut()
    //         .par_iter_mut()
    //         .enumerate()
    //         .for_each(|(i, x)| {
    //             *x = i32::try_from(i).unwrap();
    //         });
    //
    //     assert_eq!(
    //         local.thread_locals,
    //         (0..=rayon::current_num_threads()) // because + 1
    //             .map(|i| i32::try_from(i).unwrap())
    //             .collect()
    //     );
    // }
    //
    // #[test]
    // fn test_get_local_round_robin() {
    //     let mut local = RayonLocal::<i32>::init();
    //     (0..100).for_each(|_| {
    //         let thread_local = local.get_round_robin();
    //         *thread_local += 1;
    //     });
    //
    //     let sum: i32 = local.thread_locals.iter().sum();
    //     assert_eq!(sum, 100);
    //     assert!(local.thread_locals.iter().all(|&x| x > 0));
    // }
}
