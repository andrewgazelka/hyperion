#![feature(lint_reasons)]
#![feature(allocator_api)]
#![feature(portable_simd)]

// https://www.haroldserrano.com/blog/visualizing-the-boundary-volume-hierarchy-collision-algorithm

use std::{cmp::Reverse, collections::BinaryHeap, fmt::Debug, num::NonZeroI32};

use arrayvec::ArrayVec;
use glam::Vec3;

use crate::{aabb::Aabb, queue::Queue};

const MAX_ELEMENTS_PER_LEAF: usize = 8;

pub mod aabb;
pub mod plot;

mod queue;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BvhNode {
    aabb: Aabb, // f32 * 6 = 24 bytes

    // if positive then it is an internal node; if negative then it is a leaf node
    // TODO: REMOVE REMOVE REMOVE OPTION IT CAN PANIC AND GET MAX PROBS
    left: Option<NonZeroI32>,
    right: Option<NonZeroI32>,
}

impl BvhNode {
    fn create_leaf(aabb: Aabb, idx_left: usize, len: usize) -> Self {
        let left = isize::try_from(idx_left).expect("failed to convert index");
        let right = isize::try_from(len).expect("failed to convert index");

        // large number
        debug_assert!(left < 999999);

        let left = left.checked_neg().expect("failed to negate index");
        // let right = right.checked_neg().expect("failed to negate index");

        let left = i32::try_from(left).expect("failed to convert index");
        let right = i32::try_from(right).expect("failed to convert index");
        // so it is not 0
        let left = left - 1;

        let left = NonZeroI32::new(left).expect("failed to create non-max index");
        let right = NonZeroI32::new(right).expect("failed to create non-max index");

        Self {
            aabb,
            left: Some(left),
            right: Some(right),
        }
    }
}

pub struct Bvh<T> {
    nodes: Vec<BvhNode>,
    elements: Vec<T>,
    root: Option<NonZeroI32>,
}

impl<T> Default for Bvh<T> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            elements: Vec::new(),
            root: None,
        }
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
    nodes: Queue<BvhNode>,
    start_slice_pointer: *const T,
}

unsafe impl<T: Send> Send for BvhBuild<T> {}
unsafe impl<T: Sync> Sync for BvhBuild<T> {}

impl<T: HasAabb + Send + Copy + Sync + Debug> Bvh<T> {
    pub fn build<H: Heuristic>(mut elements: Vec<T>) -> Self {
        let len = elements.len();

        // 1.7 works too, 2.0 is upper bound ... 1.8 is probably best
        let cap = ((len / MAX_ELEMENTS_PER_LEAF) as f64 * 4.0) as usize;
        let cap = cap.max(16);

        // let elems = Queue::new(cap);
        let nodes = Queue::new(cap);

        // elems.push(ArrayVec::new());
        nodes.push(BvhNode::DUMMY);

        // // dummy so we never get 0 index
        // // todo: could we use negative pointer? don't think this is worth it though
        // // and think the way allocations work there are problems (pointers aren't really
        // // simple like many think they are)

        // todo: this is OFTEN UB... how to fix?
        let ptr = elements.as_ptr();

        let bvh = BvhBuild {
            nodes,
            start_slice_pointer: ptr,
        };

        let root = BvhNode::build_in::<T, H>(&bvh, &mut elements);

        Self {
            nodes: bvh.nodes.into_inner(),
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

        let on = self.root()?;

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

    pub fn get_collisions(&self, target: Aabb, mut process: impl FnMut(&T)) {
        BvhIter::consume(self, target, &mut process);
    }
}

impl<T> Bvh<T> {
    fn root(&self) -> Option<Node<T>> {
        self.root.map(|root| {
            let root = root.get();

            if root < 0 {
                return Node::Leaf(&self.elements[..]);
            }

            Node::Internal(&self.nodes[root as usize])
        })
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

// // input: sorted f64
fn find_split<T: HasAabb, H: Heuristic>(elements: &[T]) -> usize {
    H::heuristic(elements)
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

        a.partial_cmp(&b).unwrap()
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
        left: None,
        right: None,
    };

    fn left<'a, T>(&self, root: &'a Bvh<T>) -> Option<&'a Self> {
        let left = self.left?;
        let left = left.get();

        if left < 0 {
            return None;
        }

        root.nodes.get(left as usize)
    }

    fn switch_children<'a, T>(
        &'a self,
        root: &'a Bvh<T>,
        mut process_children: impl FnMut(&'a Self),
        mut process_leaf: impl FnMut(&'a [T]),
    ) {
        let left_idx = unsafe { self.left.unwrap_unchecked().get() };

        if left_idx < 0 {
            let start_idx = -left_idx - 1;
            // let start_idx = usize::try_from(start_idx).expect("failed to convert index");

            let start_idx = start_idx as usize;

            let len = unsafe { self.right.unwrap_unchecked().get() } as usize;

            let elems = &root.elements[start_idx..start_idx + len];
            process_leaf(elems);
        } else {
            let left = unsafe { self.left(root).unwrap_unchecked() };
            let right = unsafe { self.right(root).unwrap_unchecked() };

            process_children(left);
            process_children(right);
        }
    }

    // impl Iterator
    fn children<'a, T>(&'a self, root: &'a Bvh<T>) -> impl Iterator<Item = Node<T>> {
        self.children_vec(root).into_iter()
    }

    fn children_vec<'a, T>(&'a self, root: &'a Bvh<T>) -> ArrayVec<Node<T>, 2> {
        if let Some(left) = self.left {
            let left = left.get();

            // leaf
            if left < 0 {
                // println!("left: {}", left);
                let start_idx = left.checked_neg().expect("failed to negate index") - 1;

                let start_idx = usize::try_from(start_idx).expect("failed to convert index");

                let len = self.right.unwrap().get() as usize;

                let elems = &root.elements[start_idx..start_idx + len];
                let mut vec = ArrayVec::new();
                vec.push(Node::Leaf(elems));
                return vec;
            }
        }

        let mut vec = ArrayVec::new();
        if let Some(left) = self.left(root) {
            vec.push(Node::Internal(left));
        }

        if let Some(right) = self.right(root) {
            vec.push(Node::Internal(right));
        }

        vec
    }

    fn right<'a, T>(&self, root: &'a Bvh<T>) -> Option<&'a Self> {
        let right = self.right?;
        let right = right.get();

        if right < 0 {
            return None;
        }

        root.nodes.get(right as usize)
    }

    #[allow(clippy::float_cmp)]
    fn build_in<T: HasAabb + Send + Copy + Sync + Debug, H: Heuristic>(
        root: &BvhBuild<T>,
        elements: &mut [T],
    ) -> Option<NonZeroI32> {
        if elements.is_empty() {
            return None;
        }

        if elements.len() <= MAX_ELEMENTS_PER_LEAF {
            // flush
            let idx_start = unsafe { elements.as_ptr().offset_from(root.start_slice_pointer) };

            let node =
                Self::create_leaf(Aabb::from(&*elements), idx_start as usize, elements.len());

            let idx = root.nodes.push(node);
            let idx = i32::try_from(idx).expect("failed to convert index");

            debug_assert!(idx > 0);

            return Some(NonZeroI32::new(idx).expect("failed to create non-max index"));
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

        let idx = root.nodes.push(node);
        let idx = i32::try_from(idx).expect("failed to convert index");

        debug_assert!(idx > 0);

        Some(NonZeroI32::new(idx).expect("failed to create non-max index"))
    }
}

struct BvhIter<'a, T> {
    bvh: &'a Bvh<T>,
    target: Aabb,
}

impl<'a, T> BvhIter<'a, T>
where
    T: HasAabb,
{
    fn consume(bvh: &'a Bvh<T>, target: Aabb, process: &mut impl FnMut(&T)) {
        let Some(root) = bvh.root() else {
            return;
        };

        let root = match root {
            Node::Internal(internal) => internal,
            Node::Leaf(leaf) => {
                for elem in leaf.iter() {
                    if elem.aabb().collides(&target) {
                        process(elem);
                    }
                }
                return;
            }
        };

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

    pub fn process(&mut self, on: &BvhNode, process: &mut impl FnMut(&T)) {
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
                        if elem.aabb().collides(&self.target) {
                            process(elem);
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
