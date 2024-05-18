#![feature(lint_reasons)]
#![feature(portable_simd)]
#![feature(allocator_api)]

// https://www.haroldserrano.com/blog/visualizing-the-boundary-volume-hierarchy-collision-algorithm

use std::{
    alloc::Allocator,
    cmp::Reverse,
    collections::BinaryHeap,
    fmt::{Debug, Formatter},
};

use arrayvec::ArrayVec;
use glam::Vec3;

use crate::aabb::Aabb;

const ELEMENTS_TO_ACTIVATE_LEAF: usize = 16;
const VOLUME_TO_ACTIVATE_LEAF: f32 = 5.0;

pub mod aabb;

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
pub struct Bvh<T, A: Allocator = std::alloc::Global> {
    nodes: Vec<BvhNode, A>,
    elements: Vec<T, A>,
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

// get number of threads that is pow of 2
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

impl<T: HasAabb + Send + Copy + Sync + Debug, A: Allocator + Clone> Bvh<T, A> {
    pub fn null_in(allocator: A) -> Self {
        Self {
            nodes: Vec::new_in(allocator.clone()),
            elements: Vec::new_in(allocator),
            root: 0,
        }
    }

    #[tracing::instrument(skip_all, fields(elements_len = elements.len()))]
    pub fn build<H: Heuristic>(mut elements: Vec<T, A>, allocator: A) -> Self {
        let max_threads = thread_count_pow2();

        let len = elements.len();

        // // 1.7 works too, 2.0 is upper bound ... 1.8 is probably best
        let capacity = ((len / ELEMENTS_TO_ACTIVATE_LEAF) as f64 * 8.0) as usize;
        let capacity = capacity.max(16);

        let mut nodes = Vec::with_capacity_in(capacity, allocator);
        nodes.resize(capacity, BvhNode::DUMMY);

        let bvh = BvhBuild {
            start_elements_ptr: elements.as_ptr(),
            start_nodes_ptr: nodes.as_ptr(),
        };

        let nodes_slice = &mut nodes[1..];

        let (root, _) = BvhNode::build_in(&bvh, &mut elements, max_threads, 0, nodes_slice);

        Self {
            nodes,
            elements,
            root,
        }
    }

    /// Returns the closest element to the target and the distance squared to it.
    pub fn get_closest(&self, target: Vec3) -> Option<(&T, f32)> {
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
                        let aabb = elem.aabb();
                        let mid = aabb.mid();
                        let dist2 = (mid - target).length_squared();

                        (elem, dist2)
                    })
                    .min_by_key(|(_, dist)| dist.to_bits())
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
                                let aabb = elem.aabb();
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

    pub fn get_collisions(&self, target: Aabb, mut process: impl FnMut(&T) -> bool) {
        BvhIter::consume(self, target, &mut process);
    }
}

impl<T, A: Allocator> Bvh<T, A> {
    fn root(&self) -> Node<T> {
        let root = self.root;
        if root < 0 {
            return Node::Leaf(&self.elements[..]);
        }

        Node::Internal(&self.nodes[root as usize])
    }
}

pub trait HasAabb {
    fn aabb(&self) -> Aabb;
}

impl HasAabb for Aabb {
    fn aabb(&self) -> Aabb {
        *self
    }
}

pub trait Heuristic {
    /// left are partitioned to the left side,
    /// middle cannot be partitioned to either, right are partitioned to the right side
    fn heuristic<T: HasAabb>(elements: &[T]) -> usize;
}

pub struct TrivialHeuristic;

impl Heuristic for TrivialHeuristic {
    fn heuristic<T: HasAabb>(elements: &[T]) -> usize {
        elements.len() / 2
    }
}

fn sort_by_largest_axis<T: HasAabb>(elements: &mut [T], aabb: &Aabb) -> u8 {
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
        let a = a.aabb().min.as_ref()[key as usize];
        let b = b.aabb().min.as_ref()[key as usize];

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

    fn left<'a, T, A: Allocator>(&self, root: &'a Bvh<T, A>) -> Option<&'a Self> {
        let left = self.left;

        if left < 0 {
            return None;
        }

        root.nodes.get(left as usize)
    }

    fn switch_children<'a, T, A: Allocator>(
        &'a self,
        root: &'a Bvh<T, A>,
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
    fn children<'a, T, A: Allocator>(
        &'a self,
        root: &'a Bvh<T, A>,
    ) -> impl Iterator<Item = Node<T>> {
        self.children_vec(root).into_iter()
    }

    fn children_vec<'a, T, A: Allocator>(&'a self, root: &'a Bvh<T, A>) -> ArrayVec<Node<T>, 2> {
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
    unsafe fn right<'a, T, A: Allocator>(&self, root: &'a Bvh<T, A>) -> &'a Self {
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
    ) -> (i32, usize)
    where
        T: HasAabb + Send + Copy + Sync + Debug,
    {
        let aabb = Aabb::from(&*elements);
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

        sort_by_largest_axis(elements, &aabb);

        let element_split_idx = elements.len() / 2;

        let (left_elems, right_elems) = elements.split_at_mut(element_split_idx);

        debug_assert!(max_threads != 0);

        let original_node_idx = nodes_idx;

        let (left, right, nodes_idx, to_set) = if max_threads == 1 {
            let start_idx = nodes_idx;
            let (left, nodes_idx) =
                Self::build_in(root, left_elems, max_threads, nodes_idx + 1, nodes);

            let (right, nodes_idx) =
                Self::build_in(root, right_elems, max_threads, nodes_idx, nodes);
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
                || Self::build_in(root, left_elems, max_threads, 0, left_nodes),
                || Self::build_in(root, right_elems, max_threads, 0, right_nodes),
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

struct BvhIter<'a, T, A: Allocator> {
    bvh: &'a Bvh<T, A>,
    target: Aabb,
}

impl<'a, T, A: Allocator> BvhIter<'a, T, A>
where
    T: HasAabb,
{
    fn consume(bvh: &'a Bvh<T, A>, target: Aabb, process: &mut impl FnMut(&T) -> bool) {
        let root = bvh.root();

        let root = match root {
            Node::Internal(internal) => internal,
            Node::Leaf(leaf) => {
                for elem in leaf.iter() {
                    if elem.aabb().collides(&target) && !process(elem) {
                        return;
                    }
                }
                return;
            }
        };

        if !root.aabb.collides(&target) {
            return;
        }

        let iter = Self { target, bvh };

        iter.process(root, process);
    }

    pub fn process(&self, on: &BvhNode, process: &mut impl FnMut(&T) -> bool) {
        let mut stack: ArrayVec<&BvhNode, 64> = ArrayVec::new();
        stack.push(on);

        while let Some(on) = stack.pop() {
            on.switch_children(
                self.bvh,
                |child| {
                    if child.aabb.collides(&self.target) {
                        stack.push(child);
                    }
                },
                |elements| {
                    for elem in elements {
                        if elem.aabb().collides(&self.target) && !process(elem) {
                            return;
                        }
                    }
                },
            );
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
