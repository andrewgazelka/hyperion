use glam::Vec2;

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Default for Aabb {
    fn default() -> Self {
        Self {
            min: Vec2::splat(f32::INFINITY),
            max: Vec2::splat(f32::NEG_INFINITY),
        }
    }
}

pub trait Containable {
    fn contains(bounding_box: &Aabb, elem: Self) -> bool;
}

impl Containable for Vec2 {
    fn contains(bounding_box: &Aabb, elem: Self) -> bool {
        let min = bounding_box.min.as_ref();
        let max = bounding_box.max.as_ref();
        let elem = elem.as_ref();

        let mut contains = 0b1u8;

        for i in 0..2 {
            contains &= u8::from(elem[i] >= min[i]);
            contains &= u8::from(elem[i] <= max[i]);
        }

        contains == 1
    }
}

impl Containable for Aabb {
    fn contains(bounding_box: &Aabb, elem: Self) -> bool {
        let this_min = bounding_box.min.as_ref();
        let this_max = bounding_box.max.as_ref();
        let other_min = elem.min.as_ref();
        let other_max = elem.max.as_ref();

        let mut contains = 0b1u8;

        for i in 0..2 {
            contains &= u8::from(other_min[i] >= this_min[i]);
            contains &= u8::from(other_max[i] <= this_max[i]);
        }
        contains == 1
    }
}

impl Aabb {
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn area(&self) -> f64 {
        let size = self.max - self.min;
        f64::from(size.x) * f64::from(size.y)
    }

    pub fn mid(&self) -> Vec2 {
        (self.min + self.max) / 2.0
    }

    pub fn expand_to_fit(&mut self, point: Vec2) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    pub fn from_points(points: &[Vec2]) -> Self {
        let mut aabb = Self::default();

        for point in points {
            aabb.expand_to_fit(*point);
        }

        aabb
    }

    pub fn intersects(&self, other: &Self) -> bool {
        let this_min = self.min.as_ref();
        let this_max = self.max.as_ref();

        let other_min = other.min.as_ref();
        let other_max = other.max.as_ref();

        let mut intersects = 0b1u8;

        #[allow(clippy::indexing_slicing)]
        for i in 0..2 {
            intersects &= u8::from(this_min[i] <= other_max[i]);
            intersects &= u8::from(this_max[i] >= other_min[i]);
        }

        intersects == 1
    }

    #[allow(clippy::same_name_method)]
    pub fn contains<A: Containable>(&self, other: A) -> bool {
        A::contains(self, other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aabb_default() {
        let aabb = Aabb::default();
        assert_eq!(aabb.min, Vec2::splat(f32::INFINITY));
        assert_eq!(aabb.max, Vec2::splat(f32::NEG_INFINITY));
    }

    #[test]
    fn test_aabb_new() {
        let min = Vec2::new(0.0, 0.0);
        let max = Vec2::new(10.0, 10.0);
        let aabb = Aabb::new(min, max);
        assert_eq!(aabb.min, min);
        assert_eq!(aabb.max, max);
    }

    #[test]
    fn test_aabb_expand_to_fit() {
        let mut aabb = Aabb::default();
        let point = Vec2::new(5.0, 5.0);
        aabb.expand_to_fit(point);
        assert_eq!(aabb.min, point);
        assert_eq!(aabb.max, point);

        let new_point = Vec2::new(10.0, 10.0);
        aabb.expand_to_fit(new_point);
        assert_eq!(aabb.min, point);
        assert_eq!(aabb.max, new_point);
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_mid() {
        let min = Vec2::new(0.0, 0.0);
        let max = Vec2::new(2.0, 2.0);
        let aabb = Aabb::new(min, max);
        let mid = aabb.mid();
        assert_eq!(mid, Vec2::splat(1.0));
    }

    #[test]
    fn test_aabb_from_points() {
        let points = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(5.0, 5.0),
        ];
        let aabb = Aabb::from_points(&points);
        assert_eq!(aabb.min, Vec2::new(0.0, 0.0));
        assert_eq!(aabb.max, Vec2::new(10.0, 10.0));
    }

    #[test]
    fn test_intersects() {
        let aabb1 = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(2.0, 2.0));
        let aabb2 = Aabb::new(Vec2::new(1.0, 1.0), Vec2::new(3.0, 3.0));
        let aabb3 = Aabb::new(Vec2::new(3.0, 3.0), Vec2::new(4.0, 4.0));

        assert!(aabb1.intersects(&aabb2));
        assert!(aabb2.intersects(&aabb1));
        assert!(!aabb1.intersects(&aabb3));
        assert!(!aabb3.intersects(&aabb1));
    }

    #[test]
    fn test_no_intersection() {
        let aabb1 = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        let aabb2 = Aabb::new(Vec2::new(2.0, 2.0), Vec2::new(3.0, 3.0));

        assert!(!aabb1.intersects(&aabb2));
        assert!(!aabb2.intersects(&aabb1));
    }

    #[test]
    fn test_intersection_on_edge() {
        let aabb1 = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        let aabb2 = Aabb::new(Vec2::new(1.0, 1.0), Vec2::new(2.0, 2.0));

        assert!(aabb1.intersects(&aabb2));
        assert!(aabb2.intersects(&aabb1));
    }

    #[test]
    fn test_intersection() {
        let aabb1 = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(3.0, 3.0));
        let aabb2 = Aabb::new(Vec2::new(1.0, 1.0), Vec2::new(2.0, 2.0));

        assert!(aabb1.intersects(&aabb2));
        assert!(aabb2.intersects(&aabb1));
    }

    #[test]
    fn test_contains_aabb() {
        let aabb1 = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(3.0, 3.0));
        let aabb2 = Aabb::new(Vec2::new(1.0, 1.0), Vec2::new(2.0, 2.0));
        let aabb3 = Aabb::new(Vec2::new(2.0, 2.0), Vec2::new(4.0, 4.0));

        assert!(aabb1.contains(aabb2));
        assert!(!aabb2.contains(aabb1));
        assert!(!aabb1.contains(aabb3));
        assert!(!aabb3.contains(aabb1));
    }

    #[test]
    fn test_contains_vec2() {
        let aabb = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(2.0, 2.0));

        assert!(aabb.contains(Vec2::new(1.0, 1.0)));
        assert!(aabb.contains(Vec2::new(0.0, 0.0)));
        assert!(aabb.contains(Vec2::new(2.0, 2.0)));
        assert!(!aabb.contains(Vec2::new(-1.0, 1.0)));
        assert!(!aabb.contains(Vec2::new(1.0, 3.0)));
    }
}
