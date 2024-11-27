//! Entity metadata.
//!
//! The base class
//!
//! ```text
//! 0 -> Byte (0)             EntityFlags
//! 1 -> VarInt (1)           AirTicks(300)
//! 2 -> TextComponent? (6)   CustomName("")
//! 3 -> bool (8)             CustomNameVisible(false)
//! 4 -> bool (8)             Silent(false)
//! 5 -> bool (8)             NoGravity(false)
//! 6 -> Pose (21)            Pose(STANDING)
//! 7 -> VarInt (1)           TicksFrozenInPowderSnow(0)
//! ```

use flecs_ecs::prelude::*;
use valence_protocol::VarInt;

use crate::{define_and_register_components, simulation::Metadata};

mod flags;
pub use flags::EntityFlags;

// Example usage:
define_and_register_components! {
    1, AirSupply -> VarInt,
    // 2, CustomName -> Option<TextComponent>,
    3, CustomNameVisible -> bool,
    4, Silent -> bool,
    5, NoGravity -> bool,
    7, TicksFrozenInPowderSnow -> VarInt,
}

// impl Default for AirSupply {
//     fn default() -> Self {
//         Self(VarInt(300))
//     }
// }
