// https://lisyarus.github.io/blog/programming/2022/12/21/quadtrees.html

// https://snorrwe.onrender.com/posts/morton-table/

use std::ops::Range;

use glam::Vec2;

use crate::{
    aaab::Aabb,
    idx::{Idx, OptionalIdx},
};

mod aaab;
mod idx;
pub mod iter;
mod nearest;

#[derive(Debug)]
pub struct Node {
    children: [[OptionalIdx; 2]; 2],
}

impl Node {
    #[allow(dead_code)]
    const fn children(&self) -> &[[OptionalIdx; 2]; 2] {
        &self.children
    }

    fn children_iter(&self) -> impl Iterator<Item = Idx> + '_ {
        self.children
            .iter()
            .flatten()
            .filter_map(|&idx| idx.try_into().ok())
    }
}

impl Default for Node {
    fn default() -> Self {
        Self {
            children: [[OptionalIdx::NONE, OptionalIdx::NONE], [
                OptionalIdx::NONE,
                OptionalIdx::NONE,
            ]],
        }
    }
}

pub struct Quadtree {
    aabb: Aabb,
    root: OptionalIdx,
    nodes: Vec<Node>,
    points: Vec<Vec2>,
    node_points_begin: Vec<Idx>,
}

#[derive(Copy, Clone)]
struct IndexSlice {
    begin: Idx,
    end: Idx,
}

impl IndexSlice {
    const fn is_empty(self) -> bool {
        self.begin == self.end
    }

    const fn len(self) -> Idx {
        self.end - self.begin
    }

    const fn new(begin: Idx, end: Idx) -> Self {
        Self { begin, end }
    }
}

#[allow(clippy::indexing_slicing)]
fn build_impl(tree: &mut Quadtree, bbox: Aabb, points_idx: IndexSlice) -> OptionalIdx {
    if points_idx.is_empty() {
        return OptionalIdx::NONE;
    }

    let result = tree.append_new_node();
    let begin = points_idx.begin as usize;
    let points = &mut tree.points[points_idx.begin as usize..points_idx.end as usize];

    if points_idx.len() == 1 {
        tree.node_points_begin[result as usize] = begin as Idx;
        return OptionalIdx::some(result);
    }

    // equal
    if points.iter().all(|p| *p == points[0]) {
        tree.node_points_begin[result as usize] = begin as Idx;
        return OptionalIdx::some(result);
    }

    tree.node_points_begin[result as usize] = begin as Idx;

    let center = bbox.mid();

    let bottom = |p: &Vec2| p.y < center.y;
    let left = |p: &Vec2| p.x < center.x;

    // todo: why need to &mut points[..]?
    let split_y = itertools::partition(&mut *points, bottom);

    let split_x_lower = itertools::partition(&mut points[..split_y], left);
    let split_x_upper = itertools::partition(&mut points[split_y..], left);

    let child00 = build_impl(
        tree,
        Aabb::new(bbox.min, center),
        // &mut points[..split_x_lower],
        IndexSlice::new(
            points_idx.begin as Idx,
            points_idx.begin + split_x_lower as Idx,
        ),
    );
    tree.get_node_mut(result).unwrap().children[0][0] = child00;

    let child01 = build_impl(
        tree,
        Aabb::new(
            Vec2::new(center.x, bbox.min.y),
            Vec2::new(bbox.max.x, center.y),
        ),
        // &mut points[split_x_lower..split_y],
        IndexSlice::new(
            points_idx.begin + split_x_lower as Idx,
            points_idx.begin + split_y as Idx,
        ),
    );
    tree.get_node_mut(result).unwrap().children[0][1] = child01;

    let child10 = build_impl(
        tree,
        Aabb::new(
            Vec2::new(bbox.min.x, center.y),
            Vec2::new(center.x, bbox.max.y),
        ),
        // &mut points[split_y..split_y + split_x_upper],
        IndexSlice::new(
            points_idx.begin + split_y as Idx,
            points_idx.begin + split_y as Idx + split_x_upper as Idx,
        ),
    );
    tree.get_node_mut(result).unwrap().children[1][0] = child10;

    let child11 = build_impl(
        tree,
        Aabb::new(center, bbox.max),
        // &mut points[split_y + split_x_upper..],
        IndexSlice::new(
            points_idx.begin + split_y as Idx + split_x_upper as Idx,
            points_idx.end,
        ),
    );
    tree.get_node_mut(result).unwrap().children[1][1] = child11;

    OptionalIdx::some(result)
}

impl Quadtree {
    #[must_use]
    pub const fn aaabb(&self) -> &Aabb {
        &self.aabb
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn get_node(&self, id: Idx) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    fn get_node_mut(&mut self, id: Idx) -> Option<&mut Node> {
        self.nodes.get_mut(id as usize)
    }

    fn append_new_node(&mut self) -> Idx {
        let result = self.nodes.len();

        self.nodes.push(Node::default());
        self.node_points_begin.push(0);

        result as Idx
    }

    #[must_use]
    fn points_range_for(&self, idx: Idx) -> Option<Range<usize>> {
        let begin = *self.node_points_begin.get(idx as usize)?;
        let end = *self.node_points_begin.get(idx as usize + 1)?;

        Some(begin as usize..end as usize)
    }

    #[must_use]
    pub fn points(&self, idx: Idx) -> Option<&[Vec2]> {
        let range = self.points_range_for(idx)?;
        #[allow(clippy::indexing_slicing)]
        Some(&self.points[range])
    }

    #[must_use]
    pub fn is_leaf(&self, idx: Idx) -> Option<bool> {
        let range = self.points_range_for(idx)?;
        let not_leaf = range.is_empty();
        Some(!not_leaf)
    }

    #[must_use]
    pub fn points_mut(&mut self, idx: Idx) -> Option<&mut [Vec2]> {
        let range = self.points_range_for(idx)?;
        #[allow(clippy::indexing_slicing)]
        Some(&mut self.points[range])
    }

    #[must_use]
    pub fn leafs(&self) -> iter::LeafNodes {
        #[allow(clippy::option_if_let_else)]
        match self.root.inner() {
            None => iter::LeafNodes::empty(self),
            Some(root) => iter::LeafNodes::new(self, root),
        }
    }

    #[must_use]
    pub fn build(points: Vec<Vec2>) -> Self {
        let aabb = Aabb::from_points(&points);

        let len = points.len();

        let mut result = Self {
            aabb,
            root: OptionalIdx::NONE,
            nodes: vec![],
            points,
            node_points_begin: vec![],
        };

        result.root = build_impl(&mut result, aabb, IndexSlice {
            begin: 0,
            end: len as Idx,
        });

        // to eliminate edge case on right edge
        result.node_points_begin.push(result.points.len() as Idx);

        result
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use glam::Vec2;

    use crate::{
        aaab::Aabb,
        idx::{Idx, OptionalIdx},
        IndexSlice, Node, Quadtree,
    };

    #[test]
    fn test_node_default() {
        let node = Node::default();
        assert_eq!(node.children(), &[
            [OptionalIdx::NONE, OptionalIdx::NONE],
            [OptionalIdx::NONE, OptionalIdx::NONE]
        ]);
    }

    #[test]
    fn test_node_children_iter() {
        let mut node = Node::default();
        node.children[0][0] = OptionalIdx::some(1);
        node.children[1][1] = OptionalIdx::some(2);

        let children: Vec<Idx> = node.children_iter().collect();
        assert_eq!(children, vec![1, 2]);
    }

    #[test]
    fn test_index_slice() {
        let slice = IndexSlice::new(0, 5);
        assert_eq!(slice.begin, 0);
        assert_eq!(slice.end, 5);
        assert_eq!(slice.len(), 5);
        assert!(!slice.is_empty());

        let empty_slice = IndexSlice::new(2, 2);
        assert!(empty_slice.is_empty());
    }

    #[test]
    fn test_quadtree_build_empty() {
        let points = vec![];
        let tree = Quadtree::build(points);

        assert_eq!(tree.nodes.len(), 0);
        assert_eq!(tree.points.len(), 0);
        assert_eq!(tree.node_points_begin.len(), 1);
        assert_eq!(tree.root, OptionalIdx::NONE);
    }

    #[test]
    fn test_quadtree_build_single_point() {
        let points = vec![Vec2::new(1.0, 2.0)];
        let tree = Quadtree::build(points);

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.points.len(), 1);

        assert_eq!(tree.node_points_begin.len(), 2);
        assert_eq!(tree.root, OptionalIdx::some(0));

        let root_points = tree.points(0).unwrap();
        assert_eq!(root_points, &[Vec2::new(1.0, 2.0)]);
    }

    #[test]
    fn test_quadtree_build_multiple_points() {
        let points = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 3.0),
            Vec2::new(4.0, 4.0),
        ];

        let tree = Quadtree::build(points);

        assert_eq!(tree.nodes.len(), 7);
        assert_eq!(tree.points.len(), 4);
        assert_eq!(tree.node_points_begin.len(), 8);
        assert_eq!(tree.root, OptionalIdx::some(0));

        let root_points = tree.points(0).unwrap();
        assert_eq!(root_points, &[]);
    }

    #[test]
    fn test_quadtree_build_equal_points() {
        let points = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 1.0),
        ];

        let tree = Quadtree::build(points);

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.points.len(), 4);
        assert_eq!(tree.node_points_begin.len(), 2);
        assert_eq!(tree.root, OptionalIdx::some(0));

        let root_points = tree.points(0).unwrap();
        assert_eq!(root_points.len(), 4);
        assert!(root_points.iter().all(|&p| p == Vec2::new(1.0, 1.0)));
    }

    #[test]
    fn test_quadtree_aabb() {
        let points = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 3.0),
            Vec2::new(4.0, 4.0),
        ];

        let tree = Quadtree::build(points);

        let expected_aabb = Aabb::new(Vec2::new(1.0, 1.0), Vec2::new(4.0, 4.0));
        assert_eq!(tree.aaabb(), &expected_aabb);
    }

    #[test]
    fn test_quadtree_points_mut() {
        let points = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 3.0),
            Vec2::new(4.0, 4.0),
        ];

        let mut tree = Quadtree::build(points);

        let leaf = tree.leafs().next().unwrap();

        let root_points = tree.points_mut(leaf).unwrap();
        root_points[0] = Vec2::new(5.0, 5.0);

        assert_eq!(tree.points(leaf).unwrap()[0], Vec2::new(5.0, 5.0));
    }

    #[test]
    fn test_quadtree_points_out_of_range() {
        let points = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 3.0),
            Vec2::new(4.0, 4.0),
        ];

        let mut tree = Quadtree::build(points);

        assert!(tree.points(100).is_none());
        assert!(tree.points_mut(100).is_none());
    }
}
