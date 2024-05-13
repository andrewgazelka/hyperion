#![feature(allocator_api)]
#![feature(lint_reasons)]
#![feature(impl_trait_in_assoc_type)]
//! A simple thread-local storage abstraction for Rayon.

extern crate core;

#[cfg(feature = "evenio")]
mod evenio;

pub mod locals;

use std::{
    alloc::{AllocError, Allocator, Layout},
    cell::UnsafeCell,
    ops::{Deref, DerefMut, Index, IndexMut},
    ptr::NonNull,
};

#[cfg(feature = "evenio")]
pub use evenio::*;
use rayon::iter::{
    plumbing::UnindexedConsumer, IntoParallelRefIterator, IntoParallelRefMutIterator,
    ParallelIterator,
};

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
    thread_locals: Box<[UnsafeCell<S>]>,
}

#[must_use]
pub fn count() -> usize {
    rayon::current_num_threads() + 1
}

impl<'a, S> IntoIterator for &'a RayonLocal<S> {
    type Item = &'a S;

    type IntoIter = impl Iterator<Item = &'a S>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<S> IntoIterator for RayonLocal<S> {
    type Item = S;

    type IntoIter = impl Iterator<Item = S>;

    fn into_iter(self) -> Self::IntoIter {
        self.thread_locals
            .into_vec()
            .into_iter()
            .map(UnsafeCell::into_inner)
    }
}

impl<'a, S> IntoIterator for &'a mut RayonLocal<S> {
    type Item = &'a mut S;

    type IntoIter = impl Iterator<Item = &'a mut S>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<S> Index<usize> for RayonLocal<S> {
    type Output = S;

    fn index(&self, index: usize) -> &Self::Output {
        unsafe { &*self.thread_locals[index].get() }
    }
}

impl<S> IndexMut<usize> for RayonLocal<S> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        unsafe { &mut *self.thread_locals[index].get() }
    }
}

unsafe impl<A: Allocator> Allocator for RayonLocal<A> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let local = unsafe { &mut *self.get_local_raw().get() };
        local.allocate(layout)
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let local = unsafe { &mut *self.get_local_raw().get() };
        local.allocate_zeroed(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let local = unsafe { &mut *self.get_local_raw().get() };
        local.deallocate(ptr, layout);
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let local = unsafe { &mut *self.get_local_raw().get() };
        local.grow(ptr, old_layout, new_layout)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let local = unsafe { &mut *self.get_local_raw().get() };
        local.grow_zeroed(ptr, old_layout, new_layout)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let local = unsafe { &mut *self.get_local_raw().get() };
        local.shrink(ptr, old_layout, new_layout)
    }

    fn by_ref(&self) -> &Self
    where
        Self: Sized,
    {
        self
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

pub struct RayonLocalIter<'a, S> {
    local: &'a mut RayonLocal<S>,
    // idx: usize,
}

impl<'a, S> ParallelIterator for RayonLocalIter<'a, S>
where
    S: Send + Sync + 'a,
{
    type Item = &'a mut S;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        rayon::iter::repeat(())
            .map(|()| {
                let locals = &*self.local.thread_locals;
                let idx = self.local.idx();
                let local = &locals[idx];
                unsafe { &mut *local.get() }
            })
            .drive_unindexed(consumer)
    }
}

impl<S: Default> RayonLocal<S> {
    /// Create a new `RayonLocal` with the default value of `S` for each thread.
    #[must_use]
    pub fn init_with_defaults() -> Self {
        let num_threads = rayon::current_num_threads();

        let thread_locals = (0..=num_threads)
            .map(|_| UnsafeCell::new(S::default()))
            .collect();

        Self { thread_locals }
    }
}

impl<S> RayonLocal<S> {
    /// Create a new `RayonLocal` with the value of `S` provided by the closure for each thread.
    pub fn init(mut f: impl FnMut() -> S) -> Self {
        let num_threads = rayon::current_num_threads();

        let thread_locals = (0..=num_threads).map(|_| UnsafeCell::new(f())).collect();

        Self { thread_locals }
    }

    pub fn init_with_index(mut f: impl FnMut(usize) -> S) -> Self {
        let num_threads = rayon::current_num_threads();

        let thread_locals = (0..=num_threads)
            .map(|idx| UnsafeCell::new(f(idx)))
            .collect();

        Self { thread_locals }
    }

    #[must_use]
    pub fn par_iter_mut<'a>(&'a mut self) -> rayon::slice::IterMut<'a, S>
    where
        &'a mut [S]: IntoParallelRefMutIterator<'a>,
        S: Send,
    {
        self.get_all_mut().par_iter_mut()
    }

    #[must_use]
    pub fn par_iter<'a>(&'a self) -> rayon::slice::Iter<'a, S>
    where
        &'a [S]: IntoParallelRefIterator<'a>,
        S: Send + Sync,
    {
        self.get_all().par_iter()
    }

    pub fn iter(&self) -> impl Iterator<Item = &S> {
        self.get_all().iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut S> {
        self.get_all_mut().iter_mut()
    }

    unsafe fn get_ref(&self) -> RayonRef<S> {
        let locals = &*self.thread_locals;
        let idx = self.idx();
        let local = &locals[idx];
        let local = unsafe { &mut *local.get() };
        RayonRef { inner: local }
    }

    pub fn map_ref<'a, T, F>(&'a self, f: F) -> RayonLocal<T>
    where
        F: Fn(&'a S) -> T,
    {
        let thread_locals = self.get_all().iter().map(f).map(UnsafeCell::new).collect();

        RayonLocal { thread_locals }
    }

    pub fn map<T, F>(self, f: F) -> RayonLocal<T>
    where
        F: Fn(S) -> T,
    {
        let thread_locals = self.thread_locals.into_vec();
        let thread_locals = thread_locals
            .into_iter()
            .map(UnsafeCell::into_inner)
            .map(f)
            .map(UnsafeCell::new)
            .collect();

        RayonLocal { thread_locals }
    }

    #[must_use]
    pub fn idx(&self) -> usize {
        // this is so the main thread will still have a place to put data
        // todo: priority—this is currently unsafe in the situation where there is another thread beyond the main thread
        rayon::current_thread_index().unwrap_or(self.thread_locals.len() - 1)
    }

    /// Get the local value for the current thread.
    ///
    /// # Panics
    /// If the current thread is not a Rayon thread.
    #[must_use]
    pub fn get_local(&self) -> &S {
        unsafe { &*self.thread_locals[self.idx()].get() }
    }

    #[must_use]
    pub fn get_local_raw(&self) -> &UnsafeCell<S> {
        &self.thread_locals[self.idx()]
    }

    #[must_use]
    pub const fn get_raw(&self, index: usize) -> &UnsafeCell<S> {
        &self.thread_locals[index]
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

    // /// Get a mutable reference to one thread-local value, using a round-robin
    // pub fn get_round_robin(&mut self) -> &mut S {
    //     let index = self.idx;
    //     self.idx = (self.idx + 1) % self.thread_locals.len();
    //     unsafe { &mut *self.thread_locals[index].get() }
    // }
    pub fn one(&mut self) -> &mut S {
        unsafe { &mut *self.thread_locals[0].get() }
    }
}

#[cfg(test)]
mod tests {
    use rayon::prelude::*;

    use super::*;
    use crate::locals::RayonIterExt;

    #[test]
    fn test_init() {
        let local = RayonLocal::<i32>::init_with_defaults();
        assert_eq!(local.thread_locals.len(), rayon::current_num_threads() + 1);
        assert!(local.get_all().iter().all(|&x| x == 0));
    }

    #[test]
    fn test_get_rayon_local() {
        let mut local = RayonLocal::<i32>::init_with_defaults();

        (0..100)
            .into_par_iter()
            .with_locals(&mut local)
            .for_each(|(mut local, i)| {
                *local += i;
            });

        let sum: i32 = local.get_all().iter().copied().sum();
        assert_eq!(sum, (0..100).sum());
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
