use geometry::aabb::Aabb;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BvhNode {
    pub aabb: Aabb, // f32 * 6 = 24 bytes

    // if positive then it is an internal node; if negative then it is a leaf node
    pub left: i32,
    pub right: i32,
}

impl BvhNode {
    #[allow(dead_code)]
    const EMPTY_LEAF: Self = Self {
        aabb: Aabb::NULL,
        left: -1,
        right: 0,
    };

    pub const fn create_leaf(aabb: Aabb, idx_left: usize, len: usize) -> Self {
        let left = idx_left as i32;
        let right = len as i32;

        let left = -left;

        let left = left - 1;

        Self { aabb, left, right }
    }
}
