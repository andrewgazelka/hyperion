use std::fmt::Display;

use crate::HasAabb;

pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Aabb {
    pub min: glam::Vec3,
    pub max: glam::Vec3,
}

impl Display for Aabb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // write [0.00, 0.00, 0.00] -> [1.00, 1.00, 1.00]
        write!(
            f,
            "[{:.2}, {:.2}, {:.2}] -> [{:.2}, {:.2}, {:.2}]",
            self.min.x, self.min.y, self.min.z, self.max.x, self.max.y, self.max.z
        )
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Ord, PartialOrd, Hash)]
pub struct CheckableAabb {
    pub min: [ordered_float::NotNan<f32>; 3],
    pub max: [ordered_float::NotNan<f32>; 3],
}

impl TryFrom<Aabb> for CheckableAabb {
    type Error = ordered_float::FloatIsNan;

    fn try_from(value: Aabb) -> Result<Self, Self::Error> {
        Ok(Self {
            min: [
                ordered_float::NotNan::new(value.min.x)?,
                ordered_float::NotNan::new(value.min.y)?,
                ordered_float::NotNan::new(value.min.z)?,
            ],
            max: [
                ordered_float::NotNan::new(value.max.x)?,
                ordered_float::NotNan::new(value.max.y)?,
                ordered_float::NotNan::new(value.max.z)?,
            ],
        })
    }
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
        let min_z = a.min.z.max(b.min.z);

        let max_x = a.max.x.min(b.max.x);
        let max_y = a.max.y.min(b.max.y);
        let max_z = a.max.z.min(b.max.z);

        // Check if there is an overlap. If any dimension does not overlap, return None.
        if min_x < max_x && min_y < max_y && min_z < max_z {
            Some(Self {
                min: glam::Vec3::new(min_x, min_y, 0.0),
                max: glam::Vec3::new(max_x, max_y, 0.0),
            })
        } else {
            None
        }
    }

    pub fn collides(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
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

    // pub fn containing<T: HasAabb>(input: &[T]) -> Self {
    //     let mut current_min = Simd::<f32, 4>::splat(f32::INFINITY);
    //     let mut current_max = Simd::<f32, 4>::splat(f32::NEG_INFINITY);
    //
    //     // optimized for neon (128 bit registers)
    //     // todo: use chunks so we can to support AVX-512 (512 bit registers)
    //     for elem in input {
    //         let elem = elem.aabb();
    //         let min = elem.min.as_ref();
    //
    //         // make it min [f32; 4]
    //         let min_ptr = min.as_ptr();
    //
    //         // todo: is this safe?
    //         let min = unsafe { std::slice::from_raw_parts(min_ptr.sub(1), 4) };
    //
    //         let min = Simd::<f32, 4>::from_slice(min);
    //         current_min = current_min.simd_min(min);
    //
    //         let max = elem.max.as_ref();
    //         let max_ptr = max.as_ptr();
    //         let max = unsafe { std::slice::from_raw_parts(max_ptr.sub(1), 4) };
    //
    //         let max = Simd::<f32, 4>::from_slice(max);
    //         current_max = current_max.simd_max(max);
    //     }
    //
    //     let current_min = current_min.as_array();
    //     let current_max = current_max.as_array();
    //
    //     let min = glam::Vec3::new(current_min[1], current_min[2], current_min[3]);
    //     let max = glam::Vec3::new(current_max[1], current_max[2], current_max[3]);
    //
    //     Self { min, max }
    // }

    pub fn containing<T: HasAabb>(input: &[T]) -> Self {
        let mut current_min = glam::Vec3::splat(f32::INFINITY);
        let mut current_max = glam::Vec3::splat(f32::NEG_INFINITY);

        for elem in input {
            let elem = elem.aabb();
            current_min = current_min.min(elem.min);
            current_max = current_max.max(elem.max);
        }

        Self {
            min: current_min,
            max: current_max,
        }
    }
}

impl<T: HasAabb> From<&[T]> for Aabb {
    fn from(elements: &[T]) -> Self {
        Self::containing(elements)
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

    #[test]
    fn containing_returns_correct_aabb_for_multiple_aabbs() {
        let aabbs = vec![
            Aabb {
                min: glam::Vec3::new(0.0, 0.0, 0.0),
                max: glam::Vec3::new(1.0, 1.0, 1.0),
            },
            Aabb {
                min: glam::Vec3::new(-1.0, -1.0, -1.0),
                max: glam::Vec3::new(2.0, 2.0, 2.0),
            },
            Aabb {
                min: glam::Vec3::new(0.5, 0.5, 0.5),
                max: glam::Vec3::new(1.5, 1.5, 1.5),
            },
        ];

        let containing_aabb = Aabb::containing(&aabbs);

        assert_eq!(containing_aabb.min, glam::Vec3::new(-1.0, -1.0, -1.0));
        assert_eq!(containing_aabb.max, glam::Vec3::new(2.0, 2.0, 2.0));
    }

    #[test]
    fn containing_returns_correct_aabb_for_single_aabb() {
        let aabbs = vec![Aabb {
            min: glam::Vec3::new(0.0, 0.0, 0.0),
            max: glam::Vec3::new(1.0, 1.0, 1.0),
        }];

        let containing_aabb = Aabb::containing(&aabbs);

        assert_eq!(containing_aabb.min, glam::Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(containing_aabb.max, glam::Vec3::new(1.0, 1.0, 1.0));
    }

    #[test]
    fn containing_returns_null_aabb_for_empty_input() {
        let aabbs: Vec<Aabb> = vec![];

        let containing_aabb = Aabb::containing(&aabbs);

        assert_eq!(
            containing_aabb.min,
            glam::Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY)
        );
        assert_eq!(
            containing_aabb.max,
            glam::Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY)
        );
    }
}
