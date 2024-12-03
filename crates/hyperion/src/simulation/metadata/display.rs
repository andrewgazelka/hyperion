// Extends Entity.
//
// Index	Type	Meaning	Default
// 8	VarInt (1)	Interpolation delay	0
// 9	VarInt (1)	Transformation interpolation duration	0
// 10	VarInt (1)	Position/Rotation interpolation duration	0
// 11	Vector3 (28)	Translation	(0.0, 0.0, 0.0)
// 12	Vector3 (28)	Scale	(1.0, 1.0, 1.0)
// 13	Quaternion (29)	Rotation left	(0.0, 0.0, 0.0, 1.0)
// 14	Quaternion (29)	Rotation right	(0.0, 0.0, 0.0, 1.0)
// 15	Byte (0)	Billboard Constraints (0 = FIXED, 1 = VERTICAL, 2 = HORIZONTAL, 3 = CENTER)	0
// 16	VarInt (1)	Brightness override (blockLight << 4 | skyLight << 20)	-1
// 17	Float (3)	View range	1.0
// 18	Float (3)	Shadow radius	0.0
// 19	Float (3)	Shadow strength	1.0
// 20	Float (3)	Width	0.0
// 21	Float (3)	Height	0.0
// 22	VarInt (1)	Glow color override	-1

use flecs_ecs::prelude::*;
use valence_protocol::VarInt;

use super::Metadata;
use crate::define_and_register_components;

// Example usage:
define_and_register_components! {
    8, InterpolationDelay -> VarInt,
    9, TransformationInterpolationDuration -> VarInt,
    10, PositionRotationInterpolationDuration -> VarInt,
    11, Translation -> glam::Vec3,
    12, Scale -> glam::Vec3,
    13, RotationLeft -> glam::Quat,
    14, RotationRight -> glam::Quat,
    15, BillboardConstraints -> u8,
    16, BrightnessOverride -> VarInt,
    17, ViewRange -> f32,
    18, ShadowRadius -> f32,
    19, ShadowStrength -> f32,
    20, Width -> f32,
    21, Height -> f32,
    22, GlowColorOverride -> VarInt,
}

impl Default for InterpolationDelay {
    fn default() -> Self {
        Self::new(VarInt(0))
    }
}

impl Default for TransformationInterpolationDuration {
    fn default() -> Self {
        Self::new(VarInt(0))
    }
}

impl Default for PositionRotationInterpolationDuration {
    fn default() -> Self {
        Self::new(VarInt(0))
    }
}

impl Default for Translation {
    fn default() -> Self {
        Self::new(glam::Vec3::ZERO)
    }
}

impl Default for Scale {
    fn default() -> Self {
        Self::new(glam::Vec3::ONE)
    }
}

impl Default for RotationLeft {
    fn default() -> Self {
        Self::new(glam::Quat::IDENTITY)
    }
}

impl Default for RotationRight {
    fn default() -> Self {
        Self::new(glam::Quat::IDENTITY)
    }
}

impl Default for BillboardConstraints {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Default for BrightnessOverride {
    fn default() -> Self {
        Self::new(VarInt(-1))
    }
}

impl Default for ViewRange {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl Default for ShadowRadius {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl Default for ShadowStrength {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl Default for Width {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl Default for Height {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl Default for GlowColorOverride {
    fn default() -> Self {
        Self::new(VarInt(-1))
    }
}
