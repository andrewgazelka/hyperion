use std::ops::{Deref, DerefMut};

use derive_more::Constructor;

use crate::simulation::metadata::StateObserver;

#[derive(Constructor, Debug)]
pub struct Observable<T> {
    inner: T,
}

impl<T> Observable<T> {
    pub fn observe<'a, 'b>(&'a mut self, tracker: &'b StateObserver) -> ObservedMut<'a, 'b, T> {
        ObservedMut {
            inner: &mut self.inner,
            tracker,
        }
    }
}

impl<T> Deref for Observable<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

struct ObservedMut<'a, 'b, T> {
    inner: &'a mut T,
    tracker: &'b StateObserver,
}

impl<T> Deref for ObservedMut<'_, '_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for ObservedMut<'_, '_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
