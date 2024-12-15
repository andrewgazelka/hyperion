#![feature(portable_simd)]
#![feature(gen_blocks)]
#![feature(coroutines)]
#![allow(clippy::redundant_pub_crate, clippy::pedantic)]

// https://www.haroldserrano.com/blog/visualizing-the-boundary-volume-hierarchy-collision-algorithm

use std::fmt::Debug;

use arrayvec::ArrayVec;
use geometry::aabb::Aabb;
use glam::Vec3;

const ELEMENTS_TO_ACTIVATE_LEAF: usize = 16;
const VOLUME_TO_ACTIVATE_LEAF: f32 = 5.0;

mod node;
use node::BvhNode;

use crate::utils::GetAabb;

mod build;
mod query;
mod utils;

#[cfg(feature = "plot")]
pub mod plot;

#[derive(Debug, Clone)]
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
