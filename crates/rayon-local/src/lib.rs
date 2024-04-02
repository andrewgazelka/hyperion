//! A simple thread-local storage abstraction for Rayon.

#[cfg(feature = "evenio")]
mod evenio;

#[cfg(feature = "evenio")]
pub use evenio::*;

/// A simple thread-local storage abstraction for Rayon.
#[cfg_attr(feature = "evenio", derive(::evenio::component::Component))]
pub struct RayonLocal<S> {
    idx: usize,
    thread_locals: Box<[S]>,
}

unsafe impl<S: Send> Send for RayonLocal<S> {}
unsafe impl<S: Send> Sync for RayonLocal<S> {}

impl<S: Default> Default for RayonLocal<S> {
    fn default() -> Self {
        Self::init()
    }
}

impl<S: Default> RayonLocal<S> {
    /// Create a new `RayonLocal` with the default value of `S` for each thread.
    #[must_use]
    pub fn init() -> Self {
        let num_threads = rayon::current_num_threads();

        let thread_locals = (0..num_threads).map(|_| S::default()).collect();

        Self {
            thread_locals,
            idx: 0,
        }
    }

    /// Get the local value for the current thread.
    ///
    /// # Panics
    /// If the current thread is not a Rayon thread.
    #[must_use]
    pub fn get_rayon_local(&self) -> &S {
        let index = rayon::current_thread_index().expect("not in a rayon thread");
        &self.thread_locals[index]
    }

    /// Get a mutable reference to all thread-local values.
    pub fn get_all_locals(&mut self) -> &mut [S] {
        &mut self.thread_locals
    }

    /// Get a mutable reference to one thread-local value, using a round-robin
    pub fn get_local_round_robin(&mut self) -> &mut S {
        let index = self.idx;
        self.idx = (self.idx + 1) % self.thread_locals.len();
        &mut self.thread_locals[index]
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use rayon::prelude::*;

    use super::*;

    #[test]
    fn test_init() {
        let local = RayonLocal::<i32>::init();
        assert_eq!(local.thread_locals.len(), rayon::current_num_threads());
        assert!(local.thread_locals.iter().all(|&x| x == 0));
    }

    #[test]
    fn test_get_rayon_local() {
        let local = RayonLocal::<Cell<i32>>::init();
        (0..100).into_par_iter().for_each(|i| {
            let thread_local = local.get_rayon_local();
            // += i
            thread_local.set(thread_local.get() + i);
        });

        let sum: i32 = local.thread_locals.iter().map(Cell::get).sum();
        assert_eq!(sum, (0..100).sum());
    }

    #[test]
    fn test_get_all_locals() {
        let mut local = RayonLocal::<i32>::init();
        local
            .get_all_locals()
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, x)| {
                *x = i32::try_from(i).unwrap();
            });

        assert_eq!(
            local.thread_locals,
            (0..rayon::current_num_threads())
                .map(|i| i32::try_from(i).unwrap())
                .collect()
        );
    }

    #[test]
    fn test_get_local_round_robin() {
        let mut local = RayonLocal::<i32>::init();
        (0..100).for_each(|_| {
            let thread_local = local.get_local_round_robin();
            *thread_local += 1;
        });

        let sum: i32 = local.thread_locals.iter().sum();
        assert_eq!(sum, 100);
        assert!(local.thread_locals.iter().all(|&x| x > 0));
    }
}
