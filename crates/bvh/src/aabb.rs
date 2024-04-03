use crate::HasAabb;

#[derive(Copy, Clone, Debug, PartialEq)]
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
        let self_min = self.min.as_ref();
        let self_max = self.max.as_ref();

        let other_min = other.min.as_ref();
        let other_max = other.max.as_ref();

        // SIMD vectorized
        let mut collide = 0b1_u8;

        for i in 0..3 {
            collide &= u8::from(self_min[i] <= other_max[i]);
            collide &= u8::from(self_max[i] >= other_min[i]);
        }

        collide == 1
    }

    pub fn collides_point(&self, point: glam::Vec3) -> bool {
        let point = point.as_ref();
        let self_min = self.min.as_ref();
        let self_max = self.max.as_ref();

        let mut collide = 0b1_u8;

        for i in 0..3 {
            collide &= u8::from(self_min[i] <= point[i]);
            collide &= u8::from(self_max[i] >= point[i]);
        }

        collide == 1
    }

    pub fn dist2(&self, point: glam::Vec3) -> f32 {
        let point = point.as_ref();
        let self_min = self.min.as_ref();
        let self_max = self.max.as_ref();

        let mut dist2 = 0.0;

        for i in 0..3 {
            dist2 += (self_min[i] - point[i]).max(0.0).powi(2);
            dist2 += (self_max[i] - point[i]).min(0.0).powi(2);
        }

        dist2
    }

    pub fn overlaps<'a, T>(
        &'a self,
        elements: impl Iterator<Item = &'a T>,
    ) -> impl Iterator<Item = &'a T>
    where
        T: HasAabb + 'a,
    {
        elements.filter(|element| self.collides(&element.aabb()))
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

    pub fn expand(mut self, amount: f32) -> Self {
        self.min -= glam::Vec3::splat(amount);
        self.max += glam::Vec3::splat(amount);
        self
    }

    pub fn expand_to_fit(&mut self, other: &Self) {
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

    pub fn containing(input: impl Iterator<Item = Self>) -> Self {
        input.fold(Self::NULL, |acc, aabb| {
            let mut acc = acc;
            acc.expand_to_fit(&aabb);
            acc
        })
    }
}

impl<T: HasAabb> From<&[T]> for Aabb {
    fn from(elements: &[T]) -> Self {
        Self::containing(elements.iter().map(|e| e.aabb()))
    }
}

#[cfg(test)]
mod tests {
    use crate::aabb::Aabb;

    #[test]
    fn test_expand_to_fit() {
        let mut aabb = Aabb {
            min: glam::Vec3::new(0.0, 0.0, 0.0),
            max: glam::Vec3::new(1.0, 1.0, 1.0),
        };

        let other = Aabb {
            min: glam::Vec3::new(-1.0, -1.0, -1.0),
            max: glam::Vec3::new(2.0, 2.0, 2.0),
        };

        aabb.expand_to_fit(&other);

        assert_eq!(aabb.min, glam::Vec3::new(-1.0, -1.0, -1.0));
        assert_eq!(aabb.max, glam::Vec3::new(2.0, 2.0, 2.0));
    }
}
