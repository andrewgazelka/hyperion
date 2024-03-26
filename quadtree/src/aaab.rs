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

impl Aabb {
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
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
}
