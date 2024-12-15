#![feature(portable_simd)]
#![feature(gen_blocks)]
#![feature(coroutines)]
#![allow(clippy::redundant_pub_crate, clippy::pedantic)]

// https://www.haroldserrano.com/blog/visualizing-the-boundary-volume-hierarchy-collision-algorithm

use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    fmt::{Debug, Formatter},
};

use arrayvec::ArrayVec;
use geometry::aabb::Aabb;
use glam::Vec3;

const ELEMENTS_TO_ACTIVATE_LEAF: usize = 16;
const VOLUME_TO_ACTIVATE_LEAF: f32 = 5.0;

pub trait GetAabb<T>: Fn(&T) -> Aabb {}

impl<T, F> GetAabb<T> for F where F: Fn(&T) -> Aabb {}

#[cfg(feature = "plot")]
pub mod plot;

#[derive(Debug, Copy, Clone, PartialEq)]
struct BvhNode {
    aabb: Aabb, // f32 * 6 = 24 bytes

    // if positive then it is an internal node; if negative then it is a leaf node
    left: i32,
    right: i32,
}

impl BvhNode {
    #[allow(dead_code)]
    const EMPTY_LEAF: Self = Self {
        aabb: Aabb::NULL,
        left: -1,
        right: 0,
    };

    const fn create_leaf(aabb: Aabb, idx_left: usize, len: usize) -> Self {
        let left = idx_left as i32;
        let right = len as i32;

        let left = -left;

        let left = left - 1;

        Self { aabb, left, right }
    }
}

#[derive(Clone)]
pub struct Bvh<T> {
    nodes: Vec<BvhNode>,
    elements: Vec<T>,
    root: i32,
}

impl<T> Default for Bvh<T> {
    fn default() -> Self {
        Self {
            nodes: vec![BvhNode::DUMMY],
            elements: Vec::new(),
            root: 0,
        }
    }
}

impl<T> Bvh<T> {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

impl<T: Debug> Debug for Bvh<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bvh")
            .field("nodes", &self.nodes)
            .field("elems", &self.elements)
            .field("root", &self.root)
            .finish()
    }
}

struct BvhBuild<T> {
    start_elements_ptr: *const T,
    start_nodes_ptr: *const BvhNode,
}

impl<T> Debug for BvhBuild<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BvhBuild")
            .field("start_elements_ptr", &self.start_elements_ptr)
            .field("start_nodes_ptr", &self.start_nodes_ptr)
            .finish()
    }
}

unsafe impl<T: Send> Send for BvhBuild<T> {}
unsafe impl<T: Sync> Sync for BvhBuild<T> {}

/// get number of threads that is pow of 2
fn thread_count_pow2() -> usize {
    let max_threads_tentative = rayon::current_num_threads();
    // let max

    // does not make sense to not have a power of two
    let mut max_threads = max_threads_tentative.next_power_of_two();

    if max_threads != max_threads_tentative {
        max_threads >>= 1;
    }

    max_threads
}

impl<T: Send + Copy + Sync + Debug> Bvh<T> {
    #[tracing::instrument(skip_all, fields(elements_len = elements.len()))]
    pub fn build<H: Heuristic>(mut elements: Vec<T>, get_aabb: &(impl GetAabb<T> + Sync)) -> Self {
        let max_threads = thread_count_pow2();

        let len = elements.len();

        // // 1.7 works too, 2.0 is upper bound ... 1.8 is probably best
        let capacity = ((len / ELEMENTS_TO_ACTIVATE_LEAF) as f64 * 8.0) as usize;

        // [A]
        let capacity = capacity.max(16);

        let mut nodes = vec![BvhNode::DUMMY; capacity];

        let bvh = BvhBuild {
            start_elements_ptr: elements.as_ptr(),
            start_nodes_ptr: nodes.as_ptr(),
        };

        #[expect(
            clippy::indexing_slicing,
            reason = "Look at [A]. The length is at least 16, so this is safe."
        )]
        let nodes_slice = &mut nodes[1..];

        let (root, _) =
            BvhNode::build_in(&bvh, &mut elements, max_threads, 0, nodes_slice, get_aabb);

        Self {
            nodes,
            elements,
            root,
        }
    }

    /// Returns the closest element to the target and the distance squared to it.
    pub fn get_closest(&self, target: Vec3, get_aabb: &impl Fn(&T) -> Aabb) -> Option<(&T, f32)> {
        struct NodeOrd<'a> {
            node: &'a BvhNode,
            dist2: f32,
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

        let mut min_dist2 = f32::MAX;
        let mut min_node = None;

        let on = self.root();

        let on = match on {
            Node::Internal(internal) => internal,
            Node::Leaf(leaf) => {
                return leaf
                    .iter()
                    .map(|elem| {
                        let aabb = get_aabb(elem);
                        let mid = aabb.mid();
                        let dist2 = (mid - target).length_squared();

                        (elem, dist2)
                    })
                    .min_by_key(|(_, dist)| dist.to_bits());
            }
        };

        // let mut stack: SmallVec<&BvhNode, 64> = SmallVec::new();
        let mut heap: BinaryHeap<_> = std::iter::once(on)
            .map(|node| Reverse(NodeOrd { node, dist2: 0.0 }))
            .collect();

        while let Some(on) = heap.pop() {
            let on = on.0.node;
            let dist2 = on.aabb.dist2(target);

            if dist2 > min_dist2 {
                break;
            }

            for child in on.children(self) {
                match child {
                    Node::Internal(internal) => {
                        heap.push(Reverse(NodeOrd {
                            node: internal,
                            dist2: internal.aabb.dist2(target),
                        }));
                    }
                    Node::Leaf(leaf) => {
                        let (elem, dist2) = leaf
                            .iter()
                            .map(|elem| {
                                let aabb = get_aabb(elem);
                                let mid = aabb.mid();
                                let dist = (mid - target).length_squared();

                                (elem, dist)
                            })
                            .min_by_key(|(_, dist)| dist.to_bits())
                            .unwrap();

                        if dist2 < min_dist2 {
                            min_dist2 = dist2;
                            min_node = Some(elem);
                        }
                    }
                }
            }
        }

        min_node.map(|elem| (elem, min_dist2))
    }

    pub fn get_collisions<'a>(
        &'a self,
        target: Aabb,
        get_aabb: impl GetAabb<T> + 'a,
    ) -> impl Iterator<Item = &'a T> + 'a {
        BvhIter::consume(self, target, get_aabb)
    }
}

impl<T> Bvh<T> {
    fn root(&self) -> Node<'_, T> {
        let root = self.root;
        if root < 0 {
            return Node::Leaf(&self.elements[..]);
        }

        Node::Internal(&self.nodes[root as usize])
    }
}

pub trait Heuristic {
    /// left are partitioned to the left side,
    /// middle cannot be partitioned to either, right are partitioned to the right side
    fn heuristic<T>(elements: &[T]) -> usize;
}

pub struct TrivialHeuristic;

impl Heuristic for TrivialHeuristic {
    fn heuristic<T>(elements: &[T]) -> usize {
        elements.len() / 2
    }
}

fn sort_by_largest_axis<T>(elements: &mut [T], aabb: &Aabb, get_aabb: &impl Fn(&T) -> Aabb) -> u8 {
    let lens = aabb.lens();
    let largest = lens.x.max(lens.y).max(lens.z);

    let len = elements.len();
    let median_idx = len / 2;

    #[expect(
        clippy::float_cmp,
        reason = "we are not modifying; we are comparing exact values"
    )]
    let key = if lens.x == largest {
        0_u8
    } else if lens.y == largest {
        1
    } else {
        2
    };

    elements.select_nth_unstable_by(median_idx, |a, b| {
        let a = get_aabb(a).min.as_ref()[key as usize];
        let b = get_aabb(b).min.as_ref()[key as usize];

        unsafe { a.partial_cmp(&b).unwrap_unchecked() }
    });

    key
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum Node<'a, T> {
    Internal(&'a BvhNode),
    Leaf(&'a [T]),
}

impl BvhNode {
    pub const DUMMY: Self = Self {
        aabb: Aabb::NULL,
        left: 0,
        right: 0,
    };

    fn left<'a, T>(&self, root: &'a Bvh<T>) -> Option<&'a Self> {
        let left = self.left;

        if left < 0 {
            return None;
        }

        root.nodes.get(left as usize)
    }

    #[allow(unused)]
    fn switch_children<'a, T>(
        &'a self,
        root: &'a Bvh<T>,
        mut process_children: impl FnMut(&'a Self),
        mut process_leaf: impl FnMut(&'a [T]),
    ) {
        let left_idx = self.left;

        if left_idx < 0 {
            let start_idx = -left_idx - 1;
            // let start_idx = usize::try_from(start_idx).expect("failed to convert index");

            let start_idx = start_idx as usize;

            let len = self.right;

            let elems = &root.elements[start_idx..start_idx + len as usize];
            process_leaf(elems);
        } else {
            let left = unsafe { self.left(root).unwrap_unchecked() };
            let right = unsafe { self.right(root) };

            process_children(left);
            process_children(right);
        }
    }

    // impl Iterator
    fn children<'a, T>(&'a self, root: &'a Bvh<T>) -> impl Iterator<Item = Node<'a, T>> {
        self.children_vec(root).into_iter()
    }

    fn children_vec<'a, T>(&'a self, root: &'a Bvh<T>) -> ArrayVec<Node<'a, T>, 2> {
        let left = self.left;

        // leaf
        if left < 0 {
            let start_idx = left.checked_neg().expect("failed to negate index") - 1;

            let start_idx = usize::try_from(start_idx).expect("failed to convert index");

            let len = self.right as usize;

            let elems = &root.elements[start_idx..start_idx + len];
            let mut vec = ArrayVec::new();
            vec.push(Node::Leaf(elems));
            return vec;
        }

        let mut vec = ArrayVec::new();
        if let Some(left) = self.left(root) {
            vec.push(Node::Internal(left));
        }

        let right = unsafe { self.right(root) };
        vec.push(Node::Internal(right));

        vec
    }

    /// Only safe to do if already checked if left exists. If left exists then right does as well.
    unsafe fn right<'a, T>(&self, root: &'a Bvh<T>) -> &'a Self {
        let right = self.right;

        debug_assert!(right > 0);

        &root.nodes[right as usize]
    }

    #[allow(clippy::float_cmp)]
    fn build_in<T>(
        root: &BvhBuild<T>,
        elements: &mut [T],
        max_threads: usize,
        nodes_idx: usize,
        nodes: &mut [Self],
        get_aabb: &(impl GetAabb<T> + Sync),
    ) -> (i32, usize)
    where
        T: Send + Copy + Sync + Debug,
    {
        // aabb that encompasses all elements
        let aabb: Aabb = elements.iter().map(get_aabb).collect();
        let volume = aabb.volume();

        if elements.len() <= ELEMENTS_TO_ACTIVATE_LEAF || volume <= VOLUME_TO_ACTIVATE_LEAF {
            // flush
            let idx_start = unsafe { elements.as_ptr().offset_from(root.start_elements_ptr) };

            let node = Self::create_leaf(aabb, idx_start as usize, elements.len());

            let set = &mut nodes[nodes_idx..=nodes_idx];
            set[0] = node;
            let idx = unsafe { set.as_ptr().offset_from(root.start_nodes_ptr) };

            let idx = idx as i32;

            debug_assert!(idx > 0);

            return (idx, nodes_idx + 1);
        }

        sort_by_largest_axis(elements, &aabb, get_aabb);

        let element_split_idx = elements.len() / 2;

        let (left_elems, right_elems) = elements.split_at_mut(element_split_idx);

        debug_assert!(max_threads != 0);

        let original_node_idx = nodes_idx;

        let (left, right, nodes_idx, to_set) = if max_threads == 1 {
            let start_idx = nodes_idx;
            let (left, nodes_idx) = Self::build_in(
                root,
                left_elems,
                max_threads,
                nodes_idx + 1,
                nodes,
                get_aabb,
            );

            let (right, nodes_idx) =
                Self::build_in(root, right_elems, max_threads, nodes_idx, nodes, get_aabb);
            let end_idx = nodes_idx;

            debug_assert!(start_idx != end_idx);

            (
                left,
                right,
                nodes_idx,
                &mut nodes[original_node_idx..=original_node_idx],
            )
        } else {
            let max_threads = max_threads >> 1;

            let (to_set, nodes) = nodes.split_at_mut(1);

            let node_split_idx = nodes.len() / 2;
            // todo: remove fastrand
            let (left_nodes, right_nodes) = match true {
                true => {
                    let (left, right) = nodes.split_at_mut(node_split_idx);
                    (left, right)
                }
                false => {
                    let (right, left) = nodes.split_at_mut(node_split_idx);
                    (left, right)
                }
            };

            let (left, right) = rayon::join(
                || Self::build_in(root, left_elems, max_threads, 0, left_nodes, get_aabb),
                || Self::build_in(root, right_elems, max_threads, 0, right_nodes, get_aabb),
            );

            (left.0, right.0, 0, to_set)
        };

        let node = Self { aabb, left, right };

        to_set[0] = node;
        let idx = unsafe { to_set.as_ptr().offset_from(root.start_nodes_ptr) };
        let idx = idx as i32;

        // trace!("internal nodes_idx {:03}", original_node_idx);

        debug_assert!(idx > 0);

        (idx, nodes_idx + 1)
    }
}

struct BvhIter<'a, T> {
    bvh: &'a Bvh<T>,
    target: Aabb,
}

impl<'a, T> BvhIter<'a, T> {
    fn consume(
        bvh: &'a Bvh<T>,
        target: Aabb,
        get_aabb: impl GetAabb<T> + 'a,
    ) -> Box<dyn Iterator<Item = &'a T> + 'a> {
        let root = bvh.root();

        let root = match root {
            Node::Internal(internal) => internal,
            Node::Leaf(leaf) => {
                for elem in leaf.iter() {
                    let aabb = get_aabb(elem);
                    if aabb.collides(&target) {
                        return Box::new(std::iter::once(elem));
                    }
                }
                return Box::new(std::iter::empty());
            }
        };

        if !root.aabb.collides(&target) {
            return Box::new(std::iter::empty());
        }

        let iter = Self { target, bvh };

        Box::new(iter.process(root, get_aabb))
    }

    #[expect(clippy::excessive_nesting, reason = "todo: fix")]
    pub fn process(
        self,
        on: &'a BvhNode,
        get_aabb: impl GetAabb<T>,
    ) -> impl Iterator<Item = &'a T> {
        gen move {
            let mut stack: ArrayVec<&'a BvhNode, 64> = ArrayVec::new();
            stack.push(on);

            while let Some(on) = stack.pop() {
                for child in on.children(self.bvh) {
                    match child {
                        Node::Internal(child) => {
                            if child.aabb.collides(&self.target) {
                                stack.push(child);
                            }
                        }
                        Node::Leaf(elements) => {
                            for elem in elements {
                                let aabb = get_aabb(elem);
                                if aabb.collides(&self.target) {
                                    yield elem;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn random_aabb(width: f32) -> Aabb {
    let min = std::array::from_fn(|_| fastrand::f32() * width);
    let min = Vec3::from_array(min);
    let max = min + Vec3::splat(1.0);

    Aabb::new(min, max)
}

pub fn create_random_elements_1(count: usize, width: f32) -> Vec<Aabb> {
    let mut elements = Vec::new();

    for _ in 0..count {
        elements.push(random_aabb(width));
    }

    elements
}

#[cfg(test)]
mod tests;
