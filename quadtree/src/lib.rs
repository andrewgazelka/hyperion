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
    parent: OptionalIdx,
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

    #[must_use]
    pub const fn parent(&self) -> Option<Idx> {
        self.parent.inner()
    }
}

impl Default for Node {
    fn default() -> Self {
        Self {
            children: [[OptionalIdx::NONE, OptionalIdx::NONE], [
                OptionalIdx::NONE,
                OptionalIdx::NONE,
            ]],
            parent: OptionalIdx::NONE,
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

pub type IndexSlice = Range<Idx>;

#[allow(clippy::indexing_slicing)]
#[allow(unused)]
fn build_impl(
    tree: &mut Quadtree,
    bbox: Aabb,
    points_idx: IndexSlice,
    parent: OptionalIdx,
) -> OptionalIdx {
    if points_idx.is_empty() {
        return OptionalIdx::NONE;
    }

    let result = tree.append_new_node(parent);
    let start = points_idx.start as usize;
    let points = &mut tree.points[points_idx.start as usize..points_idx.end as usize];

    // println!("TOP; HAVE POINTS {:?}", points);

    if points_idx.len() == 1 {
        tree.node_points_begin[result as usize] = start as Idx;
        return OptionalIdx::some(result);
    }

    // equal
    if points.iter().all(|p| *p == points[0]) {
        tree.node_points_begin[result as usize] = start as Idx;
        return OptionalIdx::some(result);
    }

    tree.node_points_begin[result as usize] = start as Idx;

    let center = bbox.mid();

    let bottom = |p: &Vec2| p.y < center.y;
    let left = |p: &Vec2| p.x < center.x;

    // len
    // println!("points: {}", points.len());
    // println!("{:?}", points);

    // todo: why need to &mut points[..]?
    let split_y = itertools::partition(&mut *points, bottom);

    // println!();
    // println!("split_y: {} @ {}", split_y, center.y);
    // println!("BOTTOM {:?}", &points[..split_y]);
    // println!("TOP {:?}", &points[split_y..]);

    debug_assert!(points[..split_y].iter().all(|p| p.y < center.y));
    debug_assert!(points[split_y..].iter().all(|p| p.y >= center.y));

    let split_x_lower = itertools::partition(&mut points[..split_y], left) as Idx;
    let split_x_upper = itertools::partition(&mut points[split_y..], left) as Idx;

    // println!();
    // println!("split_x_lower: {} @ {}", split_x_lower, center.x);
    // println!("LEFT {:?}", &points[..split_x_lower as usize]);
    // println!("RIGHT {:?}", &points[split_x_lower as usize..split_y]);

    let split_y = split_y as Idx;

    let result_some = OptionalIdx::some(result);

    let child00_idx = points_idx.start..(points_idx.start + split_x_lower);
    let child01_idx = (points_idx.start + split_x_lower)..(points_idx.start + split_y);
    let child10_idx = (points_idx.start + split_y)..(points_idx.start + split_y + split_x_upper);
    let child11_idx = (points_idx.start + split_y + split_x_upper)..points_idx.end;

    // println!("indices \n{:?}\n{:?}\n{:?}\n{:?}", child00_idx, child01_idx, child10_idx,
    // child11_idx);

    let child00 = build_impl(tree, Aabb::new(bbox.min, center), child00_idx, result_some);

    tree.get_node_mut(result).unwrap().children[0][0] = child00;

    let child01 = build_impl(
        tree,
        Aabb::new(
            Vec2::new(center.x, bbox.min.y),
            Vec2::new(bbox.max.x, center.y),
        ),
        child01_idx,
        result_some,
    );
    tree.get_node_mut(result).unwrap().children[0][1] = child01;

    let child10 = build_impl(
        tree,
        Aabb::new(
            Vec2::new(bbox.min.x, center.y),
            Vec2::new(center.x, bbox.max.y),
        ),
        child10_idx,
        result_some,
    );
    tree.get_node_mut(result).unwrap().children[1][0] = child10;

    let child11 = build_impl(tree, Aabb::new(center, bbox.max), child11_idx, result_some);
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

    #[allow(unused)]
    fn get_node_mut(&mut self, id: Idx) -> Option<&mut Node> {
        self.nodes.get_mut(id as usize)
    }

    fn append_new_node(&mut self, parent_idx: OptionalIdx) -> Idx {
        let result = self.nodes.len();

        self.nodes.push(Node {
            children: [[OptionalIdx::NONE, OptionalIdx::NONE], [
                OptionalIdx::NONE,
                OptionalIdx::NONE,
            ]],
            parent: parent_idx,
        });

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

        if range.is_empty() {
            return None;
        }

        #[allow(clippy::indexing_slicing)]
        Some(&self.points[range])
    }

    #[must_use]
    pub fn is_leaf(&self, idx: Idx) -> Option<bool> {
        let node = self.get_node(idx)?;

        if node.children_iter().count() == 0 {
            return Some(true);
        }

        Some(false)
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

        result.root = build_impl(&mut result, aabb, 0..len as Idx, OptionalIdx::NONE);

        // to eliminate edge case on right edge
        result.node_points_begin.push(result.points.len() as Idx);

        result
    }

    #[must_use]
    pub fn query_bbox(&self, bbox: &Aabb) -> Vec<Vec2> {
        let mut result = Vec::new();
        self.query_bbox_recursive(self.root, &self.aabb, bbox, &mut result);
        result
    }

    fn query_bbox_recursive(
        &self,
        node: OptionalIdx,
        node_bbox: &Aabb,
        query_bbox: &Aabb,
        result: &mut Vec<Vec2>,
    ) {
        let Some(node_idx) = node.inner() else {
            return;
        };

        if !node_bbox.intersects(query_bbox) {
            return;
        }

        if let Some(points) = self.points(node_idx) {
            for &point in points {
                if query_bbox.contains(point) {
                    result.push(point);
                }
            }
            return;
        }

        let center = node_bbox.mid();
        let child_bboxes = [
            Aabb::new(node_bbox.min, center),
            Aabb::new(
                Vec2::new(center.x, node_bbox.min.y),
                Vec2::new(node_bbox.max.x, center.y),
            ),
            Aabb::new(
                Vec2::new(node_bbox.min.x, center.y),
                Vec2::new(center.x, node_bbox.max.y),
            ),
            Aabb::new(center, node_bbox.max),
        ];

        let node = self.get_node(node_idx).unwrap();
        for (i, &child) in node.children.iter().flatten().enumerate() {
            #[allow(clippy::indexing_slicing)]
            self.query_bbox_recursive(child, &child_bboxes[i], query_bbox, result);
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use glam::Vec2;

    use crate::{
        aaab::Aabb,
        idx::{Idx, OptionalIdx},
        Node, Quadtree,
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

        let root_points = tree.points(0);
        assert!(root_points.is_none());
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

    #[test]
    fn test_parent() {
        let points = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 3.0),
            Vec2::new(4.0, 4.0),
        ];

        let tree = Quadtree::build(points);

        let leaf = tree.leafs().next().unwrap();
        let leaf_points = tree.points(leaf).unwrap();
        assert_eq!(leaf_points.len(), 1);

        let leaf = tree.get_node(leaf).unwrap();

        let parent = leaf.parent().unwrap();
        let parent = tree.get_node(parent).unwrap();

        let grandparent = parent.parent().unwrap();
        assert_eq!(grandparent, 0);
    }

    #[test]
    fn test_query_bbox_empty_tree() {
        let qt = Quadtree::build(vec![]);
        let bbox = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_query_bbox_single_point() {
        let qt = Quadtree::build(vec![Vec2::new(0.5, 0.5)]);
        let bbox = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Vec2::new(0.5, 0.5));

        let bbox = Aabb::new(Vec2::new(0.6, 0.6), Vec2::new(1.0, 1.0));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 0);
    }

    #[test]
    #[allow(clippy::cognitive_complexity)]
    fn test_query_bbox_multiple_points() {
        let points = vec![
            Vec2::new(0.25, 0.25),
            Vec2::new(0.75, 0.25),
            Vec2::new(0.25, 0.75),
            Vec2::new(0.75, 0.75),
        ];
        let qt = Quadtree::build(points.clone());

        let bbox = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 4);
        assert_eq!(result, points);

        let bbox = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(0.5, 0.5));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Vec2::new(0.25, 0.25));

        let bbox = Aabb::new(Vec2::new(0.5, 0.0), Vec2::new(1.0, 0.5));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Vec2::new(0.75, 0.25));

        let bbox = Aabb::new(Vec2::new(0.0, 0.5), Vec2::new(0.5, 1.0));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Vec2::new(0.25, 0.75));

        let bbox = Aabb::new(Vec2::new(0.5, 0.5), Vec2::new(1.0, 1.0));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Vec2::new(0.75, 0.75));

        let bbox = Aabb::new(Vec2::new(0.25, 0.25), Vec2::new(0.75, 0.75));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 4);
        assert_eq!(result, points);
    }

    #[test]
    fn test_query_bbox_large_tree() {
        let mut points = Vec::new();
        // max 65,536 because using u16
        let width = 100; // 100 * 100 = 10_000 points

        for i in 0..width {
            let x = (i as f32) / width as f32;
            for j in 0..width {
                let y = (j as f32) / width as f32;
                points.push(Vec2::new(x, y));
            }
        }
        let qt = Quadtree::build(points.clone());

        let number_points: usize = qt.leafs().map(|idx| qt.points(idx).unwrap().len()).sum();
        assert_eq!(number_points, points.len());

        let bbox = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        let result = qt.query_bbox(&bbox);

        assert_eq!(result.len(), points.len());
        for point in points {
            assert!(result.contains(&point));
        }

        let bbox = Aabb::new(
            Vec2::new(0.25 + f32::EPSILON, 0.25 + f32::EPSILON),
            Vec2::new(0.75, 0.75),
        );
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 50 * 50);

        let bbox = Aabb::new(
            Vec2::new(0.0 + f32::EPSILON, 0.0 + f32::EPSILON),
            Vec2::new(0.01, 0.01),
        );
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_query_bbox_non_overlapping() {
        let points = vec![
            Vec2::new(0.25, 0.25),
            Vec2::new(0.75, 0.25),
            Vec2::new(0.25, 0.75),
            Vec2::new(0.75, 0.75),
        ];
        let qt = Quadtree::build(points);

        let bbox = Aabb::new(Vec2::new(-1.0, -1.0), Vec2::new(-0.5, -0.5));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_query_bbox_edge_cases() {
        let points = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
        ];
        let qt = Quadtree::build(points.clone());

        let bbox = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 4);
        assert_eq!(result, points);

        let bbox = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Vec2::new(0.0, 0.0));

        let bbox = Aabb::new(Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0));
        let result = qt.query_bbox(&bbox);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Vec2::new(1.0, 1.0));
    }
}
