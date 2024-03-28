use crate::{idx::Idx, Quadtree};

pub struct LeafNodes<'a> {
    tree: &'a Quadtree,
    stack: Vec<Idx>,
}

impl<'a> LeafNodes<'a> {
    #[must_use]
    pub(crate) fn new(tree: &'a Quadtree, root: Idx) -> Self {
        let stack = vec![root];
        Self { tree, stack }
    }

    #[must_use]
    pub(crate) const fn empty(tree: &'a Quadtree) -> Self {
        Self {
            tree,
            stack: Vec::new(),
        }
    }
}

impl<'a> Iterator for LeafNodes<'a> {
    type Item = Idx;

    #[allow(clippy::unwrap_in_result)]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(idx) = self.stack.pop() {
            if self.tree.is_leaf(idx).unwrap() {
                return Some(idx);
            }

            let node = self.tree.get_node(idx).unwrap();
            for child in node.children_iter() {
                self.stack.push(child);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;

    #[test]
    fn test_leaf_nodes_empty_tree() {
        let points = vec![];
        let tree = Quadtree::build(points);

        #[allow(clippy::needless_collect)]
        let leaf_nodes: Vec<Idx> = tree.leafs().collect();
        assert!(leaf_nodes.is_empty());
    }

    #[test]
    fn test_leaf_nodes_single_point() {
        let points = vec![Vec2::new(1.0, 1.0)];
        let tree = Quadtree::build(points);

        let leaf_nodes: Vec<Idx> = tree.leafs().collect();
        assert_eq!(leaf_nodes, vec![0]);
    }

    #[test]
    fn test_leaf_nodes_multiple_points() {
        let points = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 3.0),
            Vec2::new(4.0, 4.0),
        ];
        let tree = Quadtree::build(points.clone());

        let leaf_nodes: Vec<_> = tree
            .leafs()
            .flat_map(|idx| tree.points(idx).unwrap())
            .copied()
            .collect();

        assert_eq!(leaf_nodes.len(), points.len());

        for point in points {
            assert!(leaf_nodes.contains(&point));
        }
    }

    #[test]
    fn test_leaf_nodes_equal_points() {
        let points = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 1.0),
        ];
        let tree = Quadtree::build(points);

        let leaf_nodes: Vec<Idx> = tree.leafs().collect();
        assert_eq!(leaf_nodes, vec![0]);
    }

    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn test_leaf_nodes_invalid_root() {
        let points = vec![
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 3.0),
            Vec2::new(4.0, 4.0),
        ];
        let tree = Quadtree::build(points);

        let leaf_nodes = LeafNodes::new(&tree, 100);
        let _result: Vec<_> = leaf_nodes.collect();
    }
}
