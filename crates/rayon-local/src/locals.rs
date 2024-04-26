use rayon::iter::ParallelIterator;

use crate::{RayonLocal, RayonRef};

pub trait Locals<'a>: Send + Sync
where
    Self: 'a,
{
    type Output<I>: Send
    where
        I: Send + 'a,
        Self: 'a;

    type Shared: Copy + Send + Sync + 'a;

    fn to_shared(self) -> Self::Shared;

    /// # Safety
    ///
    /// - todo
    unsafe fn get<I: Send + 'a>(shared: Self::Shared, input: I) -> Self::Output<I>;
}

impl<'a, A> Locals<'a> for &'a mut RayonLocal<A> {
    type Output<I>     = (RayonRef<'a, A>, I)

    where
        I: Send + 'a,
        Self: 'a;
    type Shared = &'a RayonLocal<A>;

    fn to_shared(self) -> Self::Shared {
        self
    }

    unsafe fn get<I: Send + 'a>(shared: Self::Shared, input: I) -> Self::Output<I> {
        (shared.get_ref(), input)
    }
}

impl<'a, A, B> Locals<'a> for (&'a mut RayonLocal<A>, &'a mut RayonLocal<B>) {
    type Output<I>     = (RayonRef<'a, A>, RayonRef<'a, B>, I)

    where
        I: Send + 'a,
        Self: 'a;
    type Shared = (&'a RayonLocal<A>, &'a RayonLocal<B>);

    fn to_shared(self) -> Self::Shared {
        (self.0, self.1)
    }

    unsafe fn get<I: Send + 'a>(shared: Self::Shared, input: I) -> Self::Output<I> {
        (shared.0.get_ref(), shared.1.get_ref(), input)
    }
}

impl<'a, A, B, C> Locals<'a>
    for (
        &'a mut RayonLocal<A>,
        &'a mut RayonLocal<B>,
        &'a mut RayonLocal<C>,
    )
{
    type Output<I>     = (RayonRef<'a, A>, RayonRef<'a, B>, RayonRef<'a, C>, I)

    where
        I: Send + 'a,
        Self: 'a;
    type Shared = (&'a RayonLocal<A>, &'a RayonLocal<B>, &'a RayonLocal<C>);

    fn to_shared(self) -> Self::Shared {
        (self.0, self.1, self.2)
    }

    unsafe fn get<I: Send + 'a>(shared: Self::Shared, input: I) -> Self::Output<I> {
        (
            shared.0.get_ref(),
            shared.1.get_ref(),
            shared.2.get_ref(),
            input,
        )
    }
}

impl<'a, A, B, C, D> Locals<'a>
    for (
        &'a mut RayonLocal<A>,
        &'a mut RayonLocal<B>,
        &'a mut RayonLocal<C>,
        &'a mut RayonLocal<D>,
    )
{
    type Output<I>     = (RayonRef<'a, A>, RayonRef<'a, B>, RayonRef<'a, C>, RayonRef<'a, D>, I)

    where
        I: Send + 'a,
        Self: 'a;
    type Shared = (
        &'a RayonLocal<A>,
        &'a RayonLocal<B>,
        &'a RayonLocal<C>,
        &'a RayonLocal<D>,
    );

    fn to_shared(self) -> Self::Shared {
        (self.0, self.1, self.2, self.3)
    }

    unsafe fn get<I: Send + 'a>(shared: Self::Shared, input: I) -> Self::Output<I> {
        (
            shared.0.get_ref(),
            shared.1.get_ref(),
            shared.2.get_ref(),
            shared.3.get_ref(),
            input,
        )
    }
}

pub trait RayonIterExt: ParallelIterator {
    fn with_locals<'a, L: Locals<'a>>(
        self,
        locals: L,
    ) -> impl ParallelIterator<Item = L::Output<Self::Item>>
    where
        Self::Item: 'a,
        Self: 'a,
    {
        let shared = locals.to_shared();
        self.map(move |x| unsafe { L::get(shared, x) })
    }
}

impl<T> RayonIterExt for T where T: ParallelIterator {}
