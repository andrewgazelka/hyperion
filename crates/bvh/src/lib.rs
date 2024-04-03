#![feature(lint_reasons)]
#![feature(allocator_api)]

// https://www.haroldserrano.com/blog/visualizing-the-boundary-volume-hierarchy-collision-algorithm

use std::alloc::{Allocator, Global};

use smallvec::SmallVec;

use crate::aabb::Aabb;

pub mod aabb;

pub struct Bvh<A: Allocator> {
    aabb: Aabb,
    elements: SmallVec<Element, 4>,
    left: Option<Box<Bvh<A>, A>>,
    right: Option<Box<Bvh<A>, A>>,
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
    fn heuristic(elements: &[Element], split_idx: usize) -> f32;
}

struct DefaultHeuristic;

impl Heuristic for DefaultHeuristic {
    fn heuristic(elements: &[Element], split_idx: usize) -> f32 {
        let left = &elements[..split_idx];
        let right = &elements[split_idx..];

        let left_aabb = Aabb::from(left);
        let right_aabb = Aabb::from(right);

        left_aabb.surface_area() + right_aabb.surface_area()
    }
}

// // input: sorted f64
fn find_split(elements: &[Element]) -> usize {
    let mut min_cost = f32::MAX;
    let mut min_idx = 0;

    for split_idx in 0..elements.len() {
        let cost = DefaultHeuristic::heuristic(elements, split_idx);

        if cost < min_cost {
            min_cost = cost;
            min_idx = split_idx;
        }
    }

    min_idx
}

fn sort_by_largest_axis(elements: &mut [Element], aabb: &Aabb) -> u8 {
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
        let a = a.aabb.min.as_ref()[key as usize];
        let b = b.aabb.min.as_ref()[key as usize];

        a.partial_cmp(&b).unwrap()
    });

    key
}

impl<A> Bvh<A>
where
    A: Allocator + Clone,
{
    #[allow(clippy::float_cmp)]
    pub fn build_in(elements: &mut [Element], alloc: A) -> Self {
        let aabb = Aabb::from(&*elements);

        if elements.is_empty() {
            return Self {
                aabb,
                elements: SmallVec::default(),
                left: None,
                right: None,
            };
        }

        if elements.len() <= 4 {
            let elements = SmallVec::from_slice(elements);
            return Self {
                aabb,
                elements,
                left: None,
                right: None,
            };
        }

        let mut aabb = elements[0].aabb;

        for element in &elements[1..] {
            aabb.grow_to_include(&element.aabb);
        }

        sort_by_largest_axis(elements, &aabb);

        let split_idx = find_split(elements);

        let (left, right) = elements.split_at_mut(split_idx);

        let left = Box::new_in(Self::build_in(left, alloc.clone()), alloc.clone());
        let right = Box::new_in(Self::build_in(right, alloc.clone()), alloc);

        Self {
            aabb,
            elements: SmallVec::default(),
            left: Some(left),
            right: Some(right),
        }
    }

    pub fn get_collisions(&self, target: Aabb) -> BvhIter<A> {
        BvhIter::new(self, target)
    }

    pub const fn is_leaf(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }
}

pub struct BvhIter<'a, A: Allocator> {
    node_stack: Vec<&'a Bvh<A>>,
    element_stack: SmallVec<&'a Element, 4>,
    target: Aabb,
}

impl<'a, A: Allocator> BvhIter<'a, A> {
    fn new(bvh: &'a Bvh<A>, target: Aabb) -> Self {
        let node_stack = if bvh.aabb.collides(&target) {
            vec![bvh]
        } else {
            Vec::new()
        };

        Self {
            node_stack,
            target,
            element_stack: SmallVec::new(),
        }
    }
}

impl<'a> Iterator for BvhIter<'a, Global> {
    type Item = &'a Element;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.node_stack.pop() {
            if let Some(left) = node.left.as_deref() {
                if left.aabb.collides(&self.target) {
                    self.node_stack.push(left);
                }
            }

            if let Some(right) = node.right.as_deref() {
                if right.aabb.collides(&self.target) {
                    self.node_stack.push(right);
                }
            }

            for element in &node.elements {
                if element.aabb.collides(&self.target) {
                    self.element_stack.push(element);
                }
            }
        }

        self.element_stack.pop()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn create_element(min: [f32; 3], max: [f32; 3]) -> Element {
        Element {
            aabb: Aabb::new(min, max),
        }
    }

    #[test]
    fn test_empty_bvh() {
        let mut elements = Vec::new();
        let bvh = Bvh::build_in(&mut elements, Global);

        assert_eq!(bvh.get_collisions(Aabb::new([0.0; 3], [1.0; 3])).count(), 0);
    }

    #[test]
    fn test_single_element_bvh() {
        let mut elements = vec![create_element([0.0; 3], [1.0; 3])];
        let bvh = Bvh::build_in(&mut elements, Global);

        assert_eq!(bvh.get_collisions(Aabb::new([0.0; 3], [1.0; 3])).count(), 1);
        assert_eq!(bvh.get_collisions(Aabb::new([2.0; 3], [3.0; 3])).count(), 0);
    }

    #[test]
    fn test_multiple_elements_bvh() {
        let mut elements = vec![
            create_element([0.0; 3], [1.0; 3]),
            create_element([2.0; 3], [3.0; 3]),
            create_element([4.0; 3], [5.0; 3]),
        ];
        let bvh = Bvh::build_in(&mut elements, Global);

        assert_eq!(bvh.get_collisions(Aabb::new([0.0; 3], [1.0; 3])).count(), 1);
        assert_eq!(bvh.get_collisions(Aabb::new([2.0; 3], [3.0; 3])).count(), 1);
        assert_eq!(bvh.get_collisions(Aabb::new([4.0; 3], [5.0; 3])).count(), 1);
        assert_eq!(bvh.get_collisions(Aabb::new([6.0; 3], [7.0; 3])).count(), 0);
    }

    #[test]
    fn test_overlapping_elements_bvh() {
        let mut elements = vec![
            create_element([0.0; 3], [2.0; 3]),
            create_element([1.0; 3], [3.0; 3]),
            create_element([2.0; 3], [4.0; 3]),
        ];
        let bvh = Bvh::build_in(&mut elements, Global);

        assert_eq!(bvh.get_collisions(Aabb::new([0.0; 3], [1.0; 3])).count(), 1);
        assert_eq!(bvh.get_collisions(Aabb::new([1.0; 3], [2.0; 3])).count(), 2);
        assert_eq!(bvh.get_collisions(Aabb::new([2.0; 3], [3.0; 3])).count(), 2);
        assert_eq!(bvh.get_collisions(Aabb::new([3.0; 3], [4.0; 3])).count(), 1);
    }

    #[test]
    fn test_large_bvh() {
        let mut elements = Vec::new();

        for i in 0..1000 {
            let min = [i as f32; 3];
            let max = [i as f32 + 1.0; 3];
            elements.push(create_element(min, max));
        }

        let bvh = Bvh::build_in(&mut elements, Global);

        for i in 0..1000 {
            let min = [i as f32; 3];
            let max = [i as f32 + 1.0; 3];
            let target = Aabb::new(min, max);
            assert_eq!(bvh.get_collisions(target).count(), 1);
        }

        assert_eq!(
            bvh.get_collisions(Aabb::new([1000.0; 3], [1001.0; 3]))
                .count(),
            0
        );
    }
}
