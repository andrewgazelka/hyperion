// https://lisyarus.github.io/blog/programming/2022/12/21/quadtrees.html

// https://snorrwe.onrender.com/posts/morton-table/

use glam::Vec2;

mod nearest;

struct Aabb {
    min: Vec2,
    max: Vec2,
}

impl Default for Aabb {
    fn default() -> Self {
        Self {
            min: Vec2::splat(f32::INFINITY),
            max: Vec2::splat(f32::NEG_INFINITY),
        }
    }
}

impl Aabb {
    fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    fn mid(&self) -> Vec2 {
        (self.min + self.max) / 2.0
    }

    fn expand_to_fit(&mut self, point: Vec2) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    fn from_points(points: &[Vec2]) -> Self {
        let mut aabb = Self::default();

        for point in points {
            aabb.expand_to_fit(*point);
        }

        aabb
    }
}

// primitive
// struct Node {
//     children: [Option<Box<Node>>; 4],
// }
//
// struct Quadtree {
//     root: Node,
//     aabb: Aabb,
// }

struct NodeId(u32);

const NULL_ID: u32 = u32::MAX;

impl NodeId {
    const NULL: Self = Self(NULL_ID);

    const fn is_null(&self) -> bool {
        self.0 == NULL_ID
    }
}

impl From<usize> for NodeId {
    fn from(value: usize) -> Self {
        Self(value as u32)
    }
}

struct Node {
    children: [[NodeId; 2]; 2],
}

impl Default for Node {
    fn default() -> Self {
        Self {
            children: [[NodeId::NULL, NodeId::NULL], [NodeId::NULL, NodeId::NULL]],
        }
    }
}

struct Quadtree {
    aabb: Aabb,
    root: NodeId,
    nodes: Vec<Node>,
}

impl Default for Quadtree {
    fn default() -> Self {
        Self {
            aabb: Aabb::default(),
            root: NodeId::NULL,
            nodes: Vec::new(),
        }
    }
}

#[allow(clippy::indexing_slicing)]
fn build_impl(tree: &mut Quadtree, bbox: Aabb, points: &mut [Vec2]) -> NodeId {
    if points.is_empty() {
        return NodeId::NULL;
    }

    let result = tree.nodes.len();
    tree.nodes.push(Node::default());

    if points.len() == 1 {
        return result.into();
    }

    let center = bbox.mid();

    let bottom = |p: &Vec2| p.y < center.y;
    let left = |p: &Vec2| p.x < center.x;

    // todo: why need to &mut points[..]?
    let split_y = itertools::partition(&mut *points, bottom);

    let split_x_lower = itertools::partition(&mut points[..split_y], left);
    let split_x_upper = itertools::partition(&mut points[split_y..], left);

    // let node = &mut tree.nodes[result];

    let child00 = build_impl(
        tree,
        Aabb::new(bbox.min, center),
        &mut points[..split_x_lower],
    );
    tree.nodes[result].children[0][0] = child00;

    let child01 = build_impl(
        tree,
        Aabb::new(
            Vec2::new(center.x, bbox.min.y),
            Vec2::new(bbox.max.x, center.y),
        ),
        &mut points[split_x_lower..split_y],
    );
    tree.nodes[result].children[0][1] = child01;

    let child10 = build_impl(
        tree,
        Aabb::new(
            Vec2::new(bbox.min.x, center.y),
            Vec2::new(center.x, bbox.max.y),
        ),
        &mut points[split_y..split_y + split_x_upper],
    );
    tree.nodes[result].children[1][0] = child10;

    let child11 = build_impl(
        tree,
        Aabb::new(center, bbox.max),
        &mut points[split_y + split_x_upper..],
    );
    tree.nodes[result].children[1][1] = child11;

    result.into()
}

impl Quadtree {
    fn build<P>(points: &mut [Vec2]) -> Self {
        let aabb = Aabb::from_points(points);
        let mut quadtree = Self::default();

        let root = build_impl(&mut quadtree, aabb, points);

        quadtree.root = root;

        quadtree
    }
}
