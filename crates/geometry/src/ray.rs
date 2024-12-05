use glam::{IVec3, Vec3};

#[derive(Debug, Clone, Copy)]
pub struct Ray {
    origin: Vec3,
    direction: Vec3,
    inv_direction: Vec3,
}

impl Ray {
    #[must_use]
    pub const fn origin(&self) -> Vec3 {
        self.origin
    }

    #[must_use]
    pub const fn direction(&self) -> Vec3 {
        self.direction
    }

    #[must_use]
    pub const fn inv_direction(&self) -> Vec3 {
        self.inv_direction
    }

    #[must_use]
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        // Normalize direction and compute inverse
        let direction = direction.normalize();
        let inv_direction = Vec3::new(1.0 / direction.x, 1.0 / direction.y, 1.0 / direction.z);

        Self {
            origin,
            direction,
            inv_direction,
        }
    }

    /// Get the point along the ray at distance t
    #[must_use]
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }

    /// Efficiently traverse through grid cells that the ray intersects using an optimized DDA algorithm.
    /// Returns an iterator over the grid cells ([`IVec3`]) that the ray passes through.
    pub fn voxel_traversal(&self, bounds_min: IVec3, bounds_max: IVec3) -> VoxelTraversal {
        // Convert ray origin to grid coordinates and handle negative coordinates correctly
        let current_pos = if self.origin.x < 0.0 || self.origin.y < 0.0 || self.origin.z < 0.0 {
            IVec3::new(
                self.origin.x.floor() as i32,
                self.origin.y.floor() as i32,
                self.origin.z.floor() as i32,
            )
        } else {
            self.origin.as_ivec3()
        };

        // Calculate step direction for each axis
        let step = IVec3::new(
            if self.direction.x > 0.0 {
                1
            } else if self.direction.x < 0.0 {
                -1
            } else {
                0
            },
            if self.direction.y > 0.0 {
                1
            } else if self.direction.y < 0.0 {
                -1
            } else {
                0
            },
            if self.direction.z > 0.0 {
                1
            } else if self.direction.z < 0.0 {
                -1
            } else {
                0
            },
        );

        // Calculate t_max - distance to next voxel boundary for each axis
        let t_max = Vec3::new(
            if self.direction.x == 0.0 {
                f32::INFINITY
            } else {
                let next_x = if self.direction.x > 0.0 {
                    current_pos.x as f32 + 1.0 - self.origin.x
                } else {
                    self.origin.x - current_pos.x as f32
                };
                next_x * self.inv_direction.x.abs()
            },
            if self.direction.y == 0.0 {
                f32::INFINITY
            } else {
                let next_y = if self.direction.y > 0.0 {
                    current_pos.y as f32 + 1.0 - self.origin.y
                } else {
                    self.origin.y - current_pos.y as f32
                };
                next_y * self.inv_direction.y.abs()
            },
            if self.direction.z == 0.0 {
                f32::INFINITY
            } else {
                let next_z = if self.direction.z > 0.0 {
                    current_pos.z as f32 + 1.0 - self.origin.z
                } else {
                    self.origin.z - current_pos.z as f32
                };
                next_z * self.inv_direction.z.abs()
            },
        );

        // Calculate t_delta - distance between voxel boundaries
        let t_delta = Vec3::new(
            if self.direction.x == 0.0 {
                f32::INFINITY
            } else {
                self.inv_direction.x.abs()
            },
            if self.direction.y == 0.0 {
                f32::INFINITY
            } else {
                self.inv_direction.y.abs()
            },
            if self.direction.z == 0.0 {
                f32::INFINITY
            } else {
                self.inv_direction.z.abs()
            },
        );

        VoxelTraversal {
            current_pos,
            step,
            t_max,
            t_delta,
            bounds_min,
            bounds_max,
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct VoxelTraversal {
    current_pos: IVec3,
    step: IVec3,
    t_max: Vec3,
    t_delta: Vec3,
    bounds_min: IVec3,
    bounds_max: IVec3,
}

impl Iterator for VoxelTraversal {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if current position is within bounds
        if self.current_pos.x < self.bounds_min.x
            || self.current_pos.x > self.bounds_max.x
            || self.current_pos.y < self.bounds_min.y
            || self.current_pos.y > self.bounds_max.y
            || self.current_pos.z < self.bounds_min.z
            || self.current_pos.z > self.bounds_max.z
        {
            return None;
        }

        let current = self.current_pos;

        // Determine which axis to step along (the one with minimum t_max)
        if self.t_max.x < self.t_max.y {
            if self.t_max.x < self.t_max.z {
                self.current_pos.x += self.step.x;
                self.t_max.x += self.t_delta.x;
            } else {
                self.current_pos.z += self.step.z;
                self.t_max.z += self.t_delta.z;
            }
        } else if self.t_max.y < self.t_max.z {
            self.current_pos.y += self.step.y;
            self.t_max.y += self.t_delta.y;
        } else {
            self.current_pos.z += self.step.z;
            self.t_max.z += self.t_delta.z;
        }

        Some(current)
    }
}
