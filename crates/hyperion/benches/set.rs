use std::{
    collections::{BTreeSet, HashSet, LinkedList, VecDeque},
    hash::Hash,
    hint::black_box,
};

use divan::Bencher;

const LENS: &[usize] = &[1, 2, 4, 8, 16, 32, 64, 128, 256, 512];

fn main() {
    divan::main();
}
#[divan::bench(
    types = [
        BTreeSet<usize>,
        HashSet<usize>,
        Vec<usize>,
        VecDeque<usize>,
    ],
    args = LENS,
)]
fn from_iter<T>(bencher: Bencher, len: usize)
where
    T: FromIterator<usize> + Contains<usize>,
{
    let max_elem = len * 4;

    let elems: T = (0..len).map(|_| fastrand::usize(..max_elem)).collect();
    bencher.counter(len).bench_local(|| {
        let n = fastrand::usize(..max_elem);
        black_box(elems.contains_impl(&n));
    });
}

trait Contains<T> {
    fn contains_impl(&self, t: &T) -> bool;
}

impl<T: PartialEq> Contains<T> for Vec<T> {
    fn contains_impl(&self, t: &T) -> bool {
        self.contains(t)
    }
}

impl<T: PartialEq> Contains<T> for VecDeque<T> {
    fn contains_impl(&self, t: &T) -> bool {
        self.contains(t)
    }
}

impl<T: PartialEq + Ord> Contains<T> for BTreeSet<T> {
    fn contains_impl(&self, t: &T) -> bool {
        self.contains(t)
    }
}

impl<T: Eq + Hash> Contains<T> for HashSet<T> {
    fn contains_impl(&self, t: &T) -> bool {
        self.contains(t)
    }
}

impl<T: PartialEq> Contains<T> for LinkedList<T> {
    fn contains_impl(&self, t: &T) -> bool {
        self.contains(t)
    }
}
