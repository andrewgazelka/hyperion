#![feature(lint_reasons)]
#![feature(inline_const)]
#![feature(allocator_api)]
#![feature(portable_simd)]

// https://www.haroldserrano.com/blog/visualizing-the-boundary-volume-hierarchy-collision-algorithm

use std::{cmp::Reverse, collections::BinaryHeap, fmt::Debug, num::NonZeroI32};

use glam::Vec3;
use smallvec::SmallVec;

use crate::{aabb::Aabb, queue::Queue};

const MAX_ELEMENTS_PER_LEAF: usize = 16;

pub mod aabb;

mod queue;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BvhNode {
    aabb: Aabb, // f32 * 6 = 24 bytes

    // if positive then it is an internal node; if negative then it is a leaf node
    // TODO: REMOVE REMOVE REMOVE OPTION IT CAN PANIC AND GET MAX PROBS
    left: Option<NonZeroI32>,
    right: Option<NonZeroI32>,
}

pub struct Bvh<T> {
    nodes: Vec<BvhNode>,
    elems: Vec<SmallVec<T, MAX_ELEMENTS_PER_LEAF>>,
    root: Option<NonZeroI32>,
}

impl<T> Default for Bvh<T> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            elems: Vec::new(),
            root: None,
        }
    }
}

impl<T: Debug> Debug for Bvh<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bvh")
            .field("nodes", &self.nodes)
            .field("elems", &self.elems)
            .field("root", &self.root)
            .finish()
    }
}

struct BvhBuild<T> {
    nodes: Queue<BvhNode>,
    elems: Queue<SmallVec<T, MAX_ELEMENTS_PER_LEAF>>,
}

impl<T: HasAabb + Send + Copy + Sync + Debug> Bvh<T> {
    pub fn build<H: Heuristic>(elements: &mut [T]) -> Self {
        let len = elements.len();

        // 1.7 works too, 2.0 is upper bound ... 1.8 is probably best
        let cap = ((len / MAX_ELEMENTS_PER_LEAF) as f64 * 1.8) as usize;
        let cap = cap.max(16);

        let elems = Queue::new(cap);
        let nodes = Queue::new(cap);

        elems.push(SmallVec::new());
        nodes.push(BvhNode::DUMMY);

        // // dummy so we never get 0 index
        // // todo: could we use negative pointer? don't think this is worth it though
        // // and think the way allocations work there are problems (pointers aren't really
        // // simple like many think they are)

        let bvh = BvhBuild { nodes, elems };

        let root = BvhNode::build_in::<T, H>(&bvh, elements);

        Self {
            nodes: bvh.nodes.into_inner(),
            elems: bvh.elems.into_inner(),
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
                let root = root.checked_neg().expect("failed to negate index");
                return Node::Leaf(&self.elems[root as usize]);
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

// #[deprecated(note = "use TrivialHeuristic. This currently does not work properly.")]
// pub struct LeastSurfaceAreaHeuristic;

pub struct TrivialHeuristic;

impl Heuristic for TrivialHeuristic {
    fn heuristic<T: HasAabb>(elements: &[T]) -> usize {
        elements.len() / 2
    }
}

// impl Heuristic for LeastSurfaceAreaHeuristic {
//     fn heuristic<T: HasAabb>(elements: &[T]) -> usize {
//         // todo: remove new alloc each time possibly?
//         let mut left_surface_areas = vec![0.0; elements.len() - 1];
//         let mut right_surface_areas = vec![0.0; elements.len() - 1];
//
//         let mut left_bb = Aabb::NULL;
//         let mut right_bb = Aabb::NULL;
//
//         #[allow(clippy::needless_range_loop)]
//         for idx in 0..(elements.len() - 1) {
//             let left_idx = idx;
//
//             let right_idx = elements.len() - idx - 2;
//
//             left_bb.expand_to_fit(elements[left_idx].aabb());
//             right_bb.expand_to_fit(elements[right_idx].aabb());
//
//             left_surface_areas[idx] = left_bb.surface_area();
//             right_surface_areas[right_idx] = right_bb.surface_area();
//         }
//
//         // get min by summing up the surface areas
//         let mut min_cost = f32::MAX;
//         let mut min_idx = 0;
//
//         for idx in 1..elements.len() {
//             let cost = left_surface_areas[idx - 1] + right_surface_areas[idx - 1];
//
//             // // pad idx MAX_ELEMENTS_PER_LEAF zeros
//             // println!("{:04}: {}", idx, cost);
//
//             if cost < min_cost {
//                 min_cost = cost;
//                 min_idx = idx;
//             }
//         }
//
//         // assert!(min_idx != 0);
//
//         min_idx
//     }
// }

// // input: sorted f64
fn find_split<T: HasAabb, H: Heuristic>(elements: &[T]) -> usize {
    H::heuristic(elements)
}

fn sort_by_largest_axis<T: HasAabb>(elements: &mut [T], aabb: &Aabb) -> u8 {
    let lens = aabb.lens();
    let largest = lens.x.max(lens.y).max(lens.z);

    let len = elements.len();
    let median_idx = len / 2;

    #[expect(clippy::float_cmp, reason = "we are comparing exact values")]
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
    Leaf(&'a SmallVec<T, MAX_ELEMENTS_PER_LEAF>),
}

impl BvhNode {
    pub const DUMMY: Self = Self {
        aabb: Aabb::NULL,
        left: None,
        right: None,
    };

    fn left<'a, T>(&self, root: &'a Bvh<T>) -> Option<Node<'a, T>> {
        let left = self.left?;
        let left = left.get();

        if left < 0 {
            let left = left.checked_neg().expect("failed to negate index");
            return root.elems.get(left as usize).map(Node::Leaf);
        }

        root.nodes.get(left as usize).map(Node::Internal)
    }

    fn children<'a, T>(&self, root: &'a Bvh<T>) -> impl Iterator<Item = Node<'a, T>> {
        // iter of two options left and right
        let left = self.left(root);
        let right = self.right(root);

        left.into_iter().chain(right)
    }

    fn right<'a, T>(&self, root: &'a Bvh<T>) -> Option<Node<'a, T>> {
        let right = self.right?;
        let right = right.get();

        if right < 0 {
            let right = right.checked_neg().expect("failed to negate index");
            return root.elems.get(right as usize).map(Node::Leaf);
        }

        root.nodes.get(right as usize).map(Node::Internal)
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
            let elem = SmallVec::from_slice(elements);
            let idx = root.elems.push(elem);
            let idx = i32::try_from(idx).expect("failed to convert index");

            // println!("idx {idx} added leaf with len: {}", elements.len());

            debug_assert!(idx > 0);

            let idx = idx.checked_neg().expect("failed to negate index");

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
    // node_stack: Vec<&'a BvhNode>,
    bvh: &'a Bvh<T>,
    // elements: Vec<T>,
    // left_elements: Option<Entry<'a, SmallVec<T, MAX_ELEMENTS_PER_LEAF>>>,
    // right_elements: Option<Entry<'a, SmallVec<T, MAX_ELEMENTS_PER_LEAF>>>,
    target: Aabb,
}

impl<'a, T: HasAabb> BvhIter<'a, T> {
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
        // todo: ideally get .children() on same level as this
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
                            process(elem);
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
                            process(elem);
                        }
                    }
                }
            }
        }
    }
}

pub fn random_aabb() -> Aabb {
    let min = std::array::from_fn(|_| fastrand::f32() * 100.0);
    let min = Vec3::from_array(min);
    let max = min + Vec3::splat(1.0);

    Aabb::new(min, max)
}

pub fn create_random_elements_1(count: usize) -> Vec<Aabb> {
    let mut elements = Vec::new();

    for _ in 0..count {
        elements.push(random_aabb());
    }

    elements
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collisions_naive(elements: &[Aabb], target: Aabb) -> usize {
        elements
            .iter()
            .filter(|elem| elem.collides(&target))
            .count()
    }

    #[test]
    fn test_build_all_sizes() {
        let counts = &[0, 1, 10, 100];

        for count in counts {
            let mut elements = create_random_elements_1(*count);
            Bvh::build::<TrivialHeuristic>(&mut elements);
        }
    }

    #[test]
    fn test_query() {
        let mut elements = create_random_elements_1(1_000_000);
        let bvh = Bvh::build::<TrivialHeuristic>(&mut elements);

        let element = random_aabb();

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
        let bvh = Bvh::build::<TrivialHeuristic>(&mut elements);

        let node_count = bvh.nodes.len();
        println!("node count: {}", node_count);

        let mut num_collisions = 0;

        bvh.get_collisions(Aabb::EVERYTHING, |_| {
            num_collisions += 1;
        });

        assert_eq!(num_collisions, 10_000);
    }

    #[test]
    fn children_returns_none_when_no_children() {
        let node = BvhNode {
            aabb: Aabb::NULL,
            left: None,
            right: None,
        };
        let bvh: Bvh<i32> = Bvh {
            nodes: Vec::new(),
            elems: Vec::new(),
            root: None,
        };
        assert!(node.children(&bvh).next().is_none());
    }

    #[test]
    fn children_returns_internal_nodes() {
        let aabb = random_aabb();

        let child_node = BvhNode {
            aabb,
            left: None,
            right: None,
        };

        let node = BvhNode {
            aabb: aabb.expand(10.0),
            left: Some(NonZeroI32::new(1).unwrap()),
            right: Some(NonZeroI32::new(2).unwrap()),
        };

        let bvh: Bvh<i32> = Bvh {
            nodes: vec![BvhNode::DUMMY, child_node, child_node],
            elems: vec![],
            root: None,
        };
        let mut children = node.children(&bvh);
        assert_eq!(children.next(), Some(Node::Internal(&child_node)));
        assert_eq!(children.next(), Some(Node::Internal(&child_node)));
        assert!(children.next().is_none());
    }

    #[test]
    fn get_closest_returns_closest_element() {
        let mut elements = vec![
            Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
            Aabb::new(Vec3::new(2.0, 2.0, 2.0), Vec3::new(3.0, 3.0, 3.0)),
            Aabb::new(Vec3::new(4.0, 4.0, 4.0), Vec3::new(5.0, 5.0, 5.0)),
        ];
        let bvh = Bvh::build::<TrivialHeuristic>(&mut elements);

        let target = Vec3::new(2.5, 2.5, 2.5);
        let closest = bvh.get_closest(target);

        assert!(closest.is_some());
        let (closest_element, _) = closest.unwrap();
        assert_eq!(
            closest_element.aabb(),
            Aabb::new(Vec3::new(2.0, 2.0, 2.0), Vec3::new(3.0, 3.0, 3.0))
        );
    }

    #[test]
    fn get_closest_returns_closest_element_with_random_data() {
        let mut elements: Vec<Aabb> = (0..1000)
            .map(|_| {
                let min = Vec3::new(
                    fastrand::f32().mul_add(200.0, -100.0),
                    fastrand::f32().mul_add(200.0, -100.0),
                    fastrand::f32().mul_add(200.0, -100.0),
                );
                let max = min + Vec3::new(1.0, 1.0, 1.0);
                Aabb::new(min, max)
            })
            .collect();
        let bvh = Bvh::build::<TrivialHeuristic>(&mut elements);

        let target = Vec3::new(
            fastrand::f32().mul_add(200.0, -100.0),
            fastrand::f32().mul_add(200.0, -100.0),
            fastrand::f32().mul_add(200.0, -100.0),
        );
        let closest = bvh.get_closest(target);

        assert!(closest.is_some());
        let (closest_element, _) = closest.unwrap();

        // Check that the closest element is indeed the closest by comparing with all elements
        for element in &elements {
            assert!(element.aabb().dist2(target) >= closest_element.aabb().dist2(target));
        }
    }

    #[test]
    fn get_closest_returns_none_when_no_elements() {
        let mut elements: Vec<Aabb> = vec![];
        let bvh = Bvh::build::<TrivialHeuristic>(&mut elements);

        let target = Vec3::new(2.5, 2.5, 2.5);
        let closest = bvh.get_closest(target);

        assert!(closest.is_none());
    }

    #[test]
    fn children_returns_leaf_nodes() {
        let child_elems = SmallVec::<i32, MAX_ELEMENTS_PER_LEAF>::new();
        let node = BvhNode {
            aabb: Aabb::NULL,
            left: Some(NonZeroI32::new(-1).unwrap()),
            right: Some(NonZeroI32::new(-2).unwrap()),
        };
        let bvh: Bvh<i32> = Bvh {
            nodes: vec![BvhNode::DUMMY],
            elems: vec![
                SmallVec::default(),
                child_elems.clone(),
                child_elems.clone(),
            ],
            root: None,
        };
        let mut children = node.children(&bvh);
        assert_eq!(children.next(), Some(Node::Leaf(&child_elems)));
        assert_eq!(children.next(), Some(Node::Leaf(&child_elems)));
    }
}
