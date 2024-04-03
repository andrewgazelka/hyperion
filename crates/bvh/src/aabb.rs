use crate::{Element, HasAabb};

#[derive(Copy, Clone)]
pub struct Aabb {
    pub min: glam::Vec3,
    pub max: glam::Vec3,
}

impl Default for Aabb {
    fn default() -> Self {
        Self::NULL
    }
}

impl Aabb {
    pub const EVERYTHING: Self = Self {
        min: glam::Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        max: glam::Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
    };
    pub const NULL: Self = Self {
        min: glam::Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
        max: glam::Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
    };

    #[must_use]
    pub fn new(min: impl Into<glam::Vec3>, max: impl Into<glam::Vec3>) -> Self {
        let min = min.into();
        let max = max.into();
        Self { min, max }
    }

    #[must_use]
    pub fn overlap(a: &Self, b: &Self) -> Option<Self> {
        let min_x = a.min.x.max(b.min.x);
        let min_y = a.min.y.max(b.min.y);

        let max_x = a.max.x.min(b.max.x);
        let max_y = a.max.y.min(b.max.y);

        // Check if there is an overlap. If any dimension does not overlap, return None.
        if min_x < max_x && min_y < max_y {
            Some(Self {
                min: glam::Vec3::new(min_x, min_y, 0.0),
                max: glam::Vec3::new(max_x, max_y, 0.0),
            })
        } else {
            None
        }
    }

    #[must_use]
    pub fn collides(&self, other: &Self) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
    }

    pub fn overlaps<'a, T>(
        &'a self,
        elements: impl Iterator<Item = &'a T>,
    ) -> impl Iterator<Item = &'a T>
    where
        T: HasAabb + 'a,
    {
        elements.filter(|element| self.collides(element.aabb()))
    }

    #[must_use]
    pub fn surface_area(&self) -> f32 {
        let lens = self.lens();
        2.0 * lens
            .z
            .mul_add(lens.x, lens.x.mul_add(lens.y, lens.y * lens.z))
    }

    #[must_use]
    pub fn volume(&self) -> f32 {
        let lens = self.lens();
        lens.x * lens.y * lens.z
    }

    pub fn grow_to_include(&mut self, other: &Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }

    #[must_use]
    pub fn mid(&self) -> glam::Vec3 {
        (self.min + self.max) / 2.0
    }

    #[must_use]
    pub fn mid_x(&self) -> f32 {
        (self.min.x + self.max.x) / 2.0
    }

    #[must_use]
    pub fn mid_y(&self) -> f32 {
        (self.min.y + self.max.y) / 2.0
    }

    #[must_use]
    pub fn mid_z(&self) -> f32 {
        (self.min.z + self.max.z) / 2.0
    }

    #[must_use]
    pub fn lens(&self) -> glam::Vec3 {
        self.max - self.min
    }

    pub fn containing<'a>(input: impl Iterator<Item = &'a Self>) -> Self {
        input.fold(Self::NULL, |acc, aabb| {
            let mut acc = acc;
            acc.grow_to_include(aabb);
            acc
        })
    }
}

impl From<&[Element]> for Aabb {
    fn from(elements: &[Element]) -> Self {
        Self::containing(elements.iter().map(|e| &e.aabb))
    }
}

#[cfg(test)]
mod tests {
    use crate::aabb::Aabb;

    #[test]
    fn test_grow_to_fit() {
        let mut aabb = Aabb {
            min: glam::Vec3::new(0.0, 0.0, 0.0),
            max: glam::Vec3::new(1.0, 1.0, 1.0),
        };

        let other = Aabb {
            min: glam::Vec3::new(-1.0, -1.0, -1.0),
            max: glam::Vec3::new(2.0, 2.0, 2.0),
        };

        aabb.grow_to_include(&other);

        assert_eq!(aabb.min, glam::Vec3::new(-1.0, -1.0, -1.0));
        assert_eq!(aabb.max, glam::Vec3::new(2.0, 2.0, 2.0));
    }
}
