#![feature(portable_simd)]
#![feature(lint_reasons)]
#![feature(allocator_api)]

// https://www.haroldserrano.com/blog/visualizing-the-boundary-volume-hierarchy-collision-algorithm

use std::fmt::Debug;

use glam::Vec3;
use nonmax::NonMaxIsize;
use parking_lot::Mutex;
use smallvec::SmallVec;

use crate::aabb::Aabb;

const MAX_ELEMENTS_PER_LEAF: usize = 16;

pub mod aabb;

pub struct BvhNode {
    aabb: Aabb,

    // if positive then it is an internal node; if negative then it is a leaf node
    // TODO: REMOVE REMOVE REMOVE OPTION IT CAN PANIC AND GET MAX PROBS
    left: Option<NonMaxIsize>,
    right: Option<NonMaxIsize>,
}

pub struct Bvh<T> {
    nodes: Vec<BvhNode>,
    elems: Vec<SmallVec<T, MAX_ELEMENTS_PER_LEAF>>,
    root: isize,
}

impl<T: HasAabb + Copy + Send + Sync + Debug> Bvh<T> {
    pub fn build<H: Heuristic>(elements: &mut [T]) -> Self {
        let len = elements.len();

        // there will be about len / MAX_ELEMENTS_PER_LEAF nodes since max MAX_ELEMENTS_PER_LEAF
        // elements per leaf there will also be about len / 2 elems

        let nodes = Vec::with_capacity(len / MAX_ELEMENTS_PER_LEAF);
        let elems = Vec::with_capacity(len / MAX_ELEMENTS_PER_LEAF * 2);

        // // dummy so we never get 0 index
        // // todo: could we use negative pointer? don't think this is worth it though
        // // and think the way allocations work there are problems (pointers aren't really
        // // simple like many think they are)
        // elems.insert(SmallVec::new());
        // nodes.insert(BvhNode::DUMMY);

        let bvh = Self {
            nodes,
            elems,
            root: 0,
        };

        let bvh = Mutex::new(bvh);

        let root = BvhNode::build_in::<T, H>(&bvh, elements).expect("failed to build bvh");

        let mut bvh = bvh.into_inner();

        bvh.root = root.get();

        bvh
    }

    pub fn get_collisions(&self, target: Aabb, mut process: impl FnMut(T)) {
        BvhIter::consume(self, target, &mut process);
    }
}

impl<T> Bvh<T> {
    pub fn root(&self) -> &BvhNode {
        self.nodes
            .get(self.root as usize)
            .expect("failed to get root node")
    }
}

#[derive(Default, Copy, Clone, Debug)]
pub struct Element {
    pub aabb: Aabb,
}

pub trait HasAabb {
    fn aabb(&self) -> &Aabb;
}

impl HasAabb for Element {
    fn aabb(&self) -> &Aabb {
        &self.aabb
    }
}

impl HasAabb for Aabb {
    fn aabb(&self) -> &Aabb {
        self
    }
}

pub trait Heuristic {
    /// left are partitioned to the left side,
    /// middle cannot be partitioned to either, right are partitioned to the right side
    fn heuristic<T: HasAabb>(elements: &[T]) -> usize;
}

pub struct DefaultHeuristic;

pub struct TrivialHeuristic;

impl Heuristic for TrivialHeuristic {
    fn heuristic<T: HasAabb>(elements: &[T]) -> usize {
        elements.len() / 2
    }
}

impl Heuristic for DefaultHeuristic {
    fn heuristic<T: HasAabb>(elements: &[T]) -> usize {
        // todo: remove new alloc each time possibly?
        let mut left_surface_areas = vec![0.0; elements.len() - 1];
        let mut right_surface_areas = vec![0.0; elements.len() - 1];

        let mut left_bb = Aabb::NULL;
        let mut right_bb = Aabb::NULL;

        #[allow(clippy::needless_range_loop)]
        for idx in 0..(elements.len() - 1) {
            let left_idx = idx;

            let right_idx = elements.len() - idx - 2;

            left_bb.expand_to_fit(elements[left_idx].aabb());
            right_bb.expand_to_fit(elements[right_idx].aabb());

            left_surface_areas[idx] = left_bb.surface_area();
            right_surface_areas[right_idx] = right_bb.surface_area();
        }

        // get min by summing up the surface areas
        let mut min_cost = f32::MAX;
        let mut min_idx = 0;

        for idx in 1..elements.len() {
            let cost = left_surface_areas[idx - 1] + right_surface_areas[idx - 1];

            // // pad idx MAX_ELEMENTS_PER_LEAF zeros
            // println!("{:04}: {}", idx, cost);

            if cost < min_cost {
                min_cost = cost;
                min_idx = idx;
            }
        }

        // assert!(min_idx != 0);

        min_idx
    }
}

// // input: sorted f64
fn find_split<T: HasAabb, H: Heuristic>(elements: &[T]) -> usize {
    H::heuristic(elements)
}

fn sort_by_largest_axis<T: HasAabb>(elements: &mut [T], aabb: &Aabb) -> u8 {
    let lens = aabb.lens();
    let largest = lens.x.max(lens.y).max(lens.z);

    #[expect(clippy::float_cmp, reason = "we are comparing exact values")]
    let key = if lens.x == largest {
        0_u8
    } else if lens.y == largest {
        1
    } else {
        2
    };

    elements.sort_unstable_by(|a, b| {
        let a = a.aabb().min.as_ref()[key as usize];
        let b = b.aabb().min.as_ref()[key as usize];

        a.partial_cmp(&b).unwrap()
    });

    key
}

enum Node<'a, T> {
    Internal(&'a BvhNode),
    Leaf(&'a SmallVec<T, MAX_ELEMENTS_PER_LEAF>),
}

impl BvhNode {
    pub fn left<'a, T>(&self, root: &'a Bvh<T>) -> Option<Node<'a, T>> {
        let left = self.left?;
        let left = left.get();

        if left < 0 {
            let left = left.checked_neg().expect("failed to negate index") - 1;
            return root.elems.get(left as usize).map(Node::Leaf);
        }

        root.nodes.get(left as usize).map(Node::Internal)
    }

    pub fn right<'a, T>(&self, root: &'a Bvh<T>) -> Option<Node<'a, T>> {
        let right = self.right?;
        let right = right.get();

        if right < 0 {
            let right = right.checked_neg().expect("failed to negate index") - 1;
            return root.elems.get(right as usize).map(Node::Leaf);
        }

        root.nodes.get(right as usize).map(Node::Internal)
    }

    #[allow(clippy::float_cmp)]
    pub fn build_in<T: HasAabb + Copy + Send + Sync, H: Heuristic>(
        root: &Mutex<Bvh<T>>,
        elements: &mut [T],
    ) -> Option<NonMaxIsize> {
        if elements.is_empty() {
            return None;
        }

        if elements.len() <= MAX_ELEMENTS_PER_LEAF {
            let elem = SmallVec::from_slice(elements);
            let idx = {
                let mut mutex = root.lock();
                let len = mutex.elems.len();
                mutex.elems.push(elem);
                len
            };
            let idx = isize::try_from(idx).expect("failed to convert index");

            // println!("idx {idx} added leaf with len: {}", elements.len());

            debug_assert!(idx >= 0);

            let idx = idx.checked_neg().expect("failed to negate index") - 1;

            return Some(NonMaxIsize::new(idx).expect("failed to create non-max index"));
        }

        let aabb = Aabb::from(&*elements);

        sort_by_largest_axis(elements, &aabb);

        let split_idx = find_split::<T, H>(elements);

        let (left, right) = elements.split_at_mut(split_idx);

        let (left, right) = rayon::join(
            || Self::build_in::<T, H>(root, left),
            || Self::build_in::<T, H>(root, right),
        );

        let node = Self { aabb, left, right };

        let idx = {
            let mut mutex = root.lock();
            let len = mutex.nodes.len();
            mutex.nodes.push(node);
            len
        };

        let idx = idx as isize;

        debug_assert!(idx >= 0);

        Some(NonMaxIsize::new(idx).expect("failed to create non-max index"))
    }
}

pub struct BvhIter<'a, T> {
    // node_stack: Vec<&'a BvhNode>,
    bvh: &'a Bvh<T>,
    // elements: Vec<T>,
    // left_elements: Option<Entry<'a, SmallVec<T, MAX_ELEMENTS_PER_LEAF>>>,
    // right_elements: Option<Entry<'a, SmallVec<T, MAX_ELEMENTS_PER_LEAF>>>,
    target: Aabb,
}

impl<'a, T: Copy + HasAabb> BvhIter<'a, T> {
    fn consume(bvh: &'a Bvh<T>, target: Aabb, process: &mut impl FnMut(T)) {
        let root = bvh.root();

        if !root.aabb.collides(&target) {
            return;
        }

        let mut iter = Self {
            // node_stack,
            target,
            bvh,
            // elements: vec![],
        };

        iter.process(root, process);
    }

    pub fn process(&mut self, on: &BvhNode, process: &mut impl FnMut((T))) {
        if let Some(left) = on.left(self.bvh) {
            match left {
                Node::Internal(internal) => {
                    if internal.aabb.collides(&self.target) {
                        self.process(internal, process);
                    }
                }
                Node::Leaf(leaf) => {
                    for elem in leaf.iter() {
                        if elem.aabb().collides(&self.target) {
                            process(*elem);
                        }
                    }
                }
            }
        }

        if let Some(right) = on.right(self.bvh) {
            match right {
                Node::Internal(internal) => {
                    if internal.aabb.collides(&self.target) {
                        self.process(internal, process);
                    }
                }
                Node::Leaf(leaf) => {
                    for elem in leaf.iter() {
                        if elem.aabb().collides(&self.target) {
                            process(*elem);
                        }
                    }
                }
            }
        }
    }
}

pub fn random_element_1() -> Aabb {
    let min = std::array::from_fn(|_| fastrand::f32() * 100.0);
    let min = Vec3::from_array(min);
    let max = min + Vec3::splat(1.0);

    Aabb::new(min, max)
}

pub fn create_random_elements_1(count: usize) -> Vec<Aabb> {
    let mut elements = Vec::new();

    for _ in 0..count {
        elements.push(random_element_1());
    }

    elements
}

#[cfg(test)]
pub mod tests {
    use crate::{
        aabb::Aabb, create_random_elements_1, random_element_1, Bvh, HasAabb, TrivialHeuristic,
    };

    fn collisions_naive(elements: &[Aabb], target: Aabb) -> usize {
        elements
            .iter()
            .filter(|elem| elem.collides(&target))
            .count()
    }

    #[test]
    fn test_query() {
        let mut elements = create_random_elements_1(10_000_000);
        let bvh = Bvh::build::<TrivialHeuristic>(&mut elements);

        let element = random_element_1();

        let naive_count = collisions_naive(&elements, element);

        let mut num_collisions = 0;

        // 1000 x 1000 x 1000 = 1B ... 1B / 1M = 1000 blocks on average...
        // on average num_collisions should be super low
        bvh.get_collisions(element, |elem| {
            num_collisions += 1;
            assert!(elem.collides(&element));
        });

        assert_eq!(num_collisions, naive_count);
    }

    #[test]
    fn test_query_all() {
        let mut elements = create_random_elements_1(10_000);
        let mut bvh = Bvh::build::<TrivialHeuristic>(&mut elements);

        let node_count = bvh.nodes.len();
        println!("node count: {}", node_count);

        let mut num_collisions = 0;

        bvh.get_collisions(Aabb::EVERYTHING, |_| {
            num_collisions += 1;
        });

        assert_eq!(num_collisions, 10_000);
    }
}
