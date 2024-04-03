#![feature(lint_reasons)]
#![feature(allocator_api)]

// https://www.haroldserrano.com/blog/visualizing-the-boundary-volume-hierarchy-collision-algorithm

use std::fmt::Debug;

use nonmax::NonMaxIsize;
use sharded_slab::Entry;
use smallvec::SmallVec;

use crate::aabb::Aabb;

pub mod aabb;

pub struct BvhNode {
    aabb: Aabb,

    // if positive then it is an internal node; if negative then it is a leaf node
    // TODO: REMOVE REMOVE REMOVE OPTION IT CAN PANIC AND GET MAX PROBS
    left: Option<NonMaxIsize>,
    right: Option<NonMaxIsize>,
}

pub struct Bvh<T> {
    nodes: sharded_slab::Slab<BvhNode>,
    elems: sharded_slab::Slab<SmallVec<T, 4>>,
    root: isize,
}

impl<T: HasAabb + Copy + Send + Sync + Debug> Bvh<T> {
    pub fn build<H: Heuristic>(elements: &mut [T]) -> Self {
        let nodes = sharded_slab::Slab::new();
        let elems = sharded_slab::Slab::new();

        // // dummy so we never get 0 index
        // // todo: could we use negative pointer? don't think this is worth it though
        // // and think the way allocations work there are problems (pointers aren't really
        // // simple like many think they are)
        // elems.insert(SmallVec::new());
        // nodes.insert(BvhNode::DUMMY);

        let mut bvh = Self {
            nodes,
            elems,
            root: 0,
        };

        let root = BvhNode::build_in::<T, H>(&bvh, elements).expect("failed to build bvh");

        bvh.root = root.get();

        bvh
    }

    pub fn get_collisions(&self, target: Aabb) -> BvhIter<T> {
        BvhIter::new(self, target)
    }
}

impl<T> Bvh<T> {
    pub fn root(&self) -> Entry<BvhNode> {
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

            // // pad idx 4 zeros
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
    Internal(Entry<'a, BvhNode>),
    Leaf(Entry<'a, SmallVec<T, 4>>),
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
        root: &Bvh<T>,
        elements: &mut [T],
    ) -> Option<NonMaxIsize> {
        if elements.is_empty() {
            return None;
        }

        if elements.len() <= 4 {
            let elem = SmallVec::from_slice(elements);
            let idx = root.elems.insert(elem).expect("failed to insert element");
            let idx = idx as isize;

            debug_assert!(idx >= 0);

            let idx = idx.checked_neg().expect("failed to negate index") - 1;

            // println!("idx: {}", idx);

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

        let idx = root.nodes.insert(node).expect("failed to insert node");

        let idx = idx as isize;

        assert!(idx >= 0);

        Some(NonMaxIsize::new(idx).expect("failed to create non-max index"))
    }
}

pub struct BvhIter<'a, T> {
    node_stack: Vec<Entry<'a, BvhNode>>,
    bvh: &'a Bvh<T>,
    idx_left: usize,
    idx_right: usize,
    left_elements: Option<Entry<'a, SmallVec<T, 4>>>,
    right_elements: Option<Entry<'a, SmallVec<T, 4>>>,
    target: Aabb,
}

impl<'a, T> BvhIter<'a, T> {
    fn new(bvh: &'a Bvh<T>, target: Aabb) -> Self {
        let root = bvh.root();

        if !root.aabb.collides(&target) {
            return Self {
                node_stack: Vec::new(),
                bvh,
                idx_left: 0,
                idx_right: 0,
                target,
                left_elements: None,
                right_elements: None,
            };
        }

        let node_stack = vec![root];

        Self {
            node_stack,
            target,
            bvh,
            idx_left: 0,
            idx_right: 0,
            left_elements: None,
            right_elements: None,
        }
    }
}

impl<'a, T: HasAabb + Copy + Debug> Iterator for BvhIter<'a, T> {
    type Item = T;

    // todo: this loop can absolutely be improved
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(v) = &self.left_elements {
            while let Some(res) = v.get(self.idx_left) {
                self.idx_left += 1;
                // todo: is there a way to map Entry somehow so we can return a reference instead?

                if res.aabb().collides(&self.target) {
                    return Some(*res);
                }
            }
        }

        self.idx_left = 0;
        self.left_elements = None;

        if let Some(v) = &self.right_elements {
            while let Some(res) = v.get(self.idx_right) {
                self.idx_right += 1;
                // todo: is there a way to map Entry somehow so we can return a reference instead?

                if res.aabb().collides(&self.target) {
                    return Some(*res);
                }
            }
        }

        self.idx_right = 0;
        self.right_elements = None;

        if self.node_stack.is_empty() {
            return None;
        }

        if let Some(node) = self.node_stack.pop() {
            match node.right(self.bvh) {
                Some(Node::Internal(internal)) => {
                    if internal.aabb.collides(&self.target) {
                        self.node_stack.push(internal);
                    }
                }
                Some(Node::Leaf(leaf)) => {
                    self.left_elements = Some(leaf);
                }
                _ => {}
            }

            if let Some(right) = node.right(self.bvh) {
                match right {
                    Node::Internal(internal) => {
                        if internal.aabb.collides(&self.target) {
                            self.node_stack.push(internal);
                        }
                    }
                    Node::Leaf(leaf) => {
                        self.right_elements = Some(leaf);
                    }
                }
            }
        }

        self.next()
    }
}
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     fn create_element(min: [f32; 3], max: [f32; 3]) -> Element {
//         Element {
//             aabb: Aabb::new(min, max),
//         }
//     }
//
//     #[test]
//     fn test_empty_bvh() {
//         let mut elements = Vec::new();
//         let bvh = Bvh::build_in(&mut elements, Global);
//
//         assert_eq!(bvh.get_collisions(Aabb::new([0.0; 3], [1.0; 3])).count(), 0);
//     }
//
//     #[test]
//     fn test_single_element_bvh() {
//         let mut elements = vec![create_element([0.0; 3], [1.0; 3])];
//         let bvh = Bvh::build_in(&mut elements, Global);
//
//         assert_eq!(bvh.get_collisions(Aabb::new([0.0; 3], [1.0; 3])).count(), 1);
//         assert_eq!(bvh.get_collisions(Aabb::new([2.0; 3], [3.0; 3])).count(), 0);
//     }
//
//     #[test]
//     fn test_multiple_elements_bvh() {
//         let mut elements = vec![
//             create_element([0.0; 3], [1.0; 3]),
//             create_element([2.0; 3], [3.0; 3]),
//             create_element([4.0; 3], [5.0; 3]),
//         ];
//         let bvh = Bvh::build_in(&mut elements, Global);
//
//         assert_eq!(bvh.get_collisions(Aabb::new([0.0; 3], [1.0; 3])).count(), 1);
//         assert_eq!(bvh.get_collisions(Aabb::new([2.0; 3], [3.0; 3])).count(), 1);
//         assert_eq!(bvh.get_collisions(Aabb::new([4.0; 3], [5.0; 3])).count(), 1);
//         assert_eq!(bvh.get_collisions(Aabb::new([6.0; 3], [7.0; 3])).count(), 0);
//     }
//
//     #[test]
//     fn test_overlapping_elements_bvh() {
//         let mut elements = vec![
//             create_element([0.0; 3], [2.0; 3]),
//             create_element([1.0; 3], [3.0; 3]),
//             create_element([2.0; 3], [4.0; 3]),
//         ];
//         let bvh = Bvh::build_in(&mut elements, Global);
//
//         assert_eq!(bvh.get_collisions(Aabb::new([0.0; 3], [1.0; 3])).count(), 1);
//         assert_eq!(bvh.get_collisions(Aabb::new([1.0; 3], [2.0; 3])).count(), 2);
//         assert_eq!(bvh.get_collisions(Aabb::new([2.0; 3], [3.0; 3])).count(), 2);
//         assert_eq!(bvh.get_collisions(Aabb::new([3.0; 3], [4.0; 3])).count(), 1);
//     }
//
//     #[test]
//     fn test_large_bvh() {
//         let mut elements = Vec::new();
//
//         for i in 0..1000 {
//             let min = [i as f32; 3];
//             let max = [i as f32 + 1.0; 3];
//             elements.push(create_element(min, max));
//         }
//
//         let bvh = Bvh::build_in(&mut elements, Global);
//
//         for i in 0..1000 {
//             let min = [i as f32; 3];
//             let max = [i as f32 + 1.0; 3];
//             let target = Aabb::new(min, max);
//             assert_eq!(bvh.get_collisions(target).count(), 1);
//         }
//
//         assert_eq!(
//             bvh.get_collisions(Aabb::new([1000.0; 3], [1001.0; 3]))
//                 .count(),
//             0
//         );
//     }
// }

#[cfg(test)]
mod tests {
    use rand::Rng;

    use crate::{aabb::Aabb, Bvh, DefaultHeuristic, Element, HasAabb};

    fn create_element(min: [f32; 3], max: [f32; 3]) -> Element {
        Element {
            aabb: Aabb::new(min, max),
        }
    }
    // fn random_element_1() -> Element {
    //     let mut rng = rand::thread_rng();
    //     let min = [rng.gen_range(0.0..1000.0); 3];
    //     let max = [
    //         rng.gen_range(min[0]..1.0),
    //         rng.gen_range(min[1]..10.0),
    //         rng.gen_range(min[2]..1000.0),
    //     ];
    //     create_element(min, max)
    // }

    fn random_element_1() -> Element {
        let mut rng = rand::thread_rng();
        let min = [rng.gen_range(0.0..1000.0); 3];
        let max = [
            rng.gen_range(min[0]..min[0] + 1.0),
            rng.gen_range(min[1]..min[1] + 1.0),
            rng.gen_range(min[2]..min[2] + 1.0),
        ];
        create_element(min, max)
    }

    fn create_random_elements_1(count: usize) -> Vec<Element> {
        let mut elements = Vec::new();

        for _ in 0..count {
            elements.push(random_element_1());
        }

        elements
    }

    #[test]
    fn test_query() {
        let mut elements = create_random_elements_1(100_000);
        let bvh = Bvh::build::<DefaultHeuristic>(&mut elements);

        let element = random_element_1();
        for elem in bvh.get_collisions(element.aabb) {
            assert!(elem.aabb().collides(&element.aabb));
        }
    }

    #[test]
    fn test_query_all() {
        let mut elements = create_random_elements_1(100_000);
        let mut bvh = Bvh::build::<DefaultHeuristic>(&mut elements);

        let node_count = bvh.nodes.unique_iter().count();
        println!("node count: {}", node_count);

        let num_collisions = bvh.get_collisions(bvh.root().aabb).count();

        assert_eq!(num_collisions, 100_000);
    }
}
