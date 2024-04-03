#![feature(lint_reasons)]
#![feature(allocator_api)]

// https://www.haroldserrano.com/blog/visualizing-the-boundary-volume-hierarchy-collision-algorithm

use nonmax::NonMaxI32;
use sharded_slab::Entry;
use smallvec::SmallVec;

use crate::aabb::Aabb;

pub mod aabb;

pub struct BvhNode {
    aabb: Aabb,

    // if positive then it is an internal node; if negative then it is a leaf node
    left: Option<NonMaxI32>,
    right: Option<NonMaxI32>,
}

impl BvhNode {
    const DUMMY: Self = Self {
        aabb: Aabb::NULL,
        left: None,
        right: None,
    };
}

pub struct Bvh<T> {
    nodes: sharded_slab::Slab<BvhNode>,
    elems: sharded_slab::Slab<SmallVec<T, 4>>,
    root: u32,
}

impl<T: HasAabb + Copy + Send + Sync> Bvh<T> {
    pub fn build(elements: &mut [T]) -> Self {
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

        let root = BvhNode::build_in(&bvh, elements).expect("failed to build bvh");

        bvh.root = root.get() as u32;

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

#[derive(Default, Copy, Clone)]
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

trait Heuristic {
    /// left are partitioned to the left side,
    /// middle cannot be partitioned to either, right are partitioned to the right side
    fn heuristic<T: HasAabb>(elements: &[T], split_idx: usize) -> f32;
}

struct DefaultHeuristic;

impl Heuristic for DefaultHeuristic {
    fn heuristic<T: HasAabb>(elements: &[T], split_idx: usize) -> f32 {
        let left = &elements[..split_idx];
        let right = &elements[split_idx..];

        let left_aabb = Aabb::from(left);
        let right_aabb = Aabb::from(right);

        left_aabb.surface_area() + right_aabb.surface_area()
    }
}

// // input: sorted f64
const fn find_split<T: HasAabb>(elements: &[T]) -> usize {
    // let mut min_cost = f32::MAX;
    // let mut min_idx = 0;
    //
    // for split_idx in 0..elements.len() {
    //     let cost = DefaultHeuristic::heuristic(elements, split_idx);
    //
    //     if cost < min_cost {
    //         min_cost = cost;
    //         min_idx = split_idx;
    //     }
    // }

    // jank
    let min_idx = elements.len() / 2;

    min_idx
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

impl BvhNode {
    pub fn left<'a, T>(&self, root: &'a Bvh<T>) -> Option<Entry<'a, Self>> {
        let left = self.left?;

        // leaf node
        if left.get() < 0 {
            return None;
        }

        root.nodes.get(left.get() as usize)
    }

    pub fn elements<'a, T>(&self, root: &'a Bvh<T>) -> Option<Entry<'a, SmallVec<T, 4>>> {
        let idx = self.left?;

        if idx.get() > 0 {
            return None;
        }

        let idx = idx.get().checked_neg().expect("failed to negate index");

        root.elems.get(idx as usize)
    }

    pub fn right<'a, T>(&self, root: &'a Bvh<T>) -> Option<Entry<'a, Self>> {
        let right = self.right?;

        // leaf node
        if right.get() < 0 {
            return None;
        }

        root.nodes.get(right.get() as usize)
    }

    #[allow(clippy::float_cmp)]
    pub fn build_in<T: HasAabb + Copy + Send + Sync>(
        root: &Bvh<T>,
        elements: &mut [T],
    ) -> Option<NonMaxI32> {
        if elements.is_empty() {
            return None;
        }

        if elements.len() <= 4 {
            let elem = SmallVec::from_slice(elements);
            let idx = root.elems.insert(elem).expect("failed to insert element");
            let index = (idx as i32).checked_neg().expect("failed to negate index");

            return Some(NonMaxI32::new(index).expect("failed to create non-max index"));
        }

        let aabb = Aabb::from(&*elements);

        sort_by_largest_axis(elements, &aabb);

        let split_idx = find_split(elements);

        let (left, right) = elements.split_at_mut(split_idx);

        let (left, right) = rayon::join(
            || Self::build_in(root, left),
            || Self::build_in(root, right),
        );

        let node = Self { aabb, left, right };

        let idx = root.nodes.insert(node).expect("failed to insert node");

        Some(NonMaxI32::new(idx as i32).expect("failed to create non-max index"))
    }

    pub const fn is_leaf(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }
}

pub struct BvhIter<'a, T> {
    node_stack: Vec<Entry<'a, BvhNode>>,
    bvh: &'a Bvh<T>,
    idx: usize,
    elements: Option<Entry<'a, SmallVec<T, 4>>>,
    target: Aabb,
}

impl<'a, T> BvhIter<'a, T> {
    fn new(bvh: &'a Bvh<T>, target: Aabb) -> Self {
        let root = bvh.root();

        if !root.aabb.collides(&target) {
            return Self {
                node_stack: Vec::new(),
                bvh,
                idx: 0,
                target,
                elements: None,
            };
        }

        let node_stack = vec![root];

        Self {
            node_stack,
            target,
            bvh,
            idx: 0,
            elements: None,
        }
    }
}

impl<'a, T: HasAabb + Copy> Iterator for BvhIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(v) = &self.elements {
            if let Some(res) = v.get(self.idx) {
                self.idx += 1;
                // todo: is there a way to map Entry somehow so we can return a reference instead?
                return Some(*res);
            }
            self.idx = 0;
            self.elements = None;
        }

        while let Some(node) = self.node_stack.pop() {
            if let Some(left) = node.left(self.bvh) {
                if left.aabb.collides(&self.target) {
                    self.node_stack.push(left);
                }
            }

            if let Some(right) = node.right(self.bvh) {
                if right.aabb.collides(&self.target) {
                    self.node_stack.push(right);
                }
            }

            if let Some(elements) = node.elements(self.bvh) {
                self.elements = Some(elements);
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
