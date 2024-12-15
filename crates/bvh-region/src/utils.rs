use derive_more::Constructor;
use geometry::aabb::Aabb;

use crate::node::BvhNode;

/// get number of threads that is pow of 2
pub fn thread_count_pow2() -> usize {
    let max_threads_tentative = rayon::current_num_threads();
    // let max

    // does not make sense to not have a power of two
    let mut max_threads = max_threads_tentative.next_power_of_two();

    if max_threads != max_threads_tentative {
        max_threads >>= 1;
    }

    max_threads
}

pub trait GetAabb<T>: Fn(&T) -> Aabb {}

impl<T, F> GetAabb<T> for F where F: Fn(&T) -> Aabb {}

#[derive(Constructor, Copy, Clone, Debug)]
pub struct NodeOrd<'a, T> {
    pub node: &'a BvhNode,
    pub by: T,
}

impl<T: PartialEq> PartialEq<Self> for NodeOrd<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.by == other.by
    }
}
impl<T: PartialOrd> PartialOrd for NodeOrd<'_, T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.by.partial_cmp(&other.by)
    }
}

impl<T: Eq> Eq for NodeOrd<'_, T> {}

impl<T: Ord> Ord for NodeOrd<'_, T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.by.cmp(&other.by)
    }
}
