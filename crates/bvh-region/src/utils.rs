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

#[derive(Constructor)]
pub struct NodeOrd<'a> {
    pub node: &'a BvhNode,
    pub dist2: f64,
}

impl PartialEq<Self> for NodeOrd<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.dist2 == other.dist2
    }
}
impl PartialOrd for NodeOrd<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for NodeOrd<'_> {}

impl Ord for NodeOrd<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.dist2.partial_cmp(&other.dist2).unwrap()
    }
}
