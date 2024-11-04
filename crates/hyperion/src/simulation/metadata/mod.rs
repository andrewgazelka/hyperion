use derive_more::Deref;
use flecs_ecs::macros::Component;
use valence_protocol::{Encode, VarInt};

use crate::simulation::metadata::r#type::MetadataType;

#[derive(Debug, Default)]
// index (u8), type (varint), value (varies)
/// <https://wiki.vg/Entity_metadata>
///
/// Tracks updates within a gametick for the metadata
pub struct MetadataBuilder(Vec<u8>);

unsafe impl Send for MetadataBuilder {}

// technically not Sync but I mean do we really care? todo: Indra
unsafe impl Sync for MetadataBuilder {}

mod status;

mod r#type;

pub trait Metadata {
    const INDEX: u8;
    type Type: MetadataType + Encode;
    fn to_type(self) -> Self::Type;
}

// todo: can be u8
#[derive(Component, PartialEq, Eq, Copy, Clone, Debug, Deref)]
pub struct EntityFlags {
    value: u8,
}

impl Default for EntityFlags {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityFlags {
    pub const CROUCHING: Self = Self { value: 0x02 };
    pub const FLYING_WITH_ELYTRA: Self = Self { value: 0x80 };
    pub const GLOWING: Self = Self { value: 0x40 };
    pub const INVISIBLE: Self = Self { value: 0x20 };
    pub const ON_FIRE: Self = Self { value: 0x01 };
    // 0x04 skipped (previously riding)
    pub const SPRINTING: Self = Self { value: 0x08 };
    pub const SWIMMING: Self = Self { value: 0x10 };

    const fn new() -> Self {
        Self { value: 0 }
    }
}

impl std::ops::BitOrAssign for EntityFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.value |= rhs.value;
    }
}

impl std::ops::BitAndAssign for EntityFlags {
    fn bitand_assign(&mut self, rhs: Self) {
        self.value &= rhs.value;
    }
}

impl std::ops::BitOr for EntityFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self {
            value: self.value | rhs.value,
        }
    }
}

impl std::ops::BitAnd for EntityFlags {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self {
        Self {
            value: self.value & rhs.value,
        }
    }
}

impl std::ops::Not for EntityFlags {
    type Output = Self;

    fn not(self) -> Self {
        Self { value: !self.value }
    }
}

impl Metadata for EntityFlags {
    type Type = u8;

    const INDEX: u8 = 0;

    fn to_type(self) -> Self::Type {
        self.value
    }
}

// Air supply component
#[derive(Component, Default)]
pub struct AirSupply {
    pub ticks: i32, // VarInt in original, using i32 for Rust
}

impl Metadata for AirSupply {
    type Type = VarInt;

    const INDEX: u8 = 1;

    fn to_type(self) -> Self::Type {
        VarInt(self.ticks)
    }
}

#[derive(Encode, Clone, Copy, Default, PartialEq, Eq, Debug)]
#[repr(u8)]
#[derive(Component)]
pub enum Pose {
    #[default]
    Standing,
    FallFlying,
    Sleeping,
    Swimming,
    SpinAttack,
    Sneaking,
    LongJumping,
    Dying,
    Croaking,
    UsingTongue,
    Sitting,
    Roaring,
    Sniffing,
    Emerging,
    Digging,
}

impl Metadata for Pose {
    type Type = Self;

    const INDEX: u8 = 6;

    fn to_type(self) -> Self::Type {
        self
    }
}

impl MetadataBuilder {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn encode<M: Metadata>(&mut self, metadata: M) {
        let value_index = M::INDEX;
        self.0.push(value_index);

        let type_index = VarInt(<M as Metadata>::Type::INDEX);
        type_index.encode(&mut self.0).unwrap();

        let r#type = metadata.to_type();
        r#type.encode(&mut self.0).unwrap();
    }

    pub fn get_and_clear(&mut self) -> Option<MetadataView<'_>> {
        if self.is_empty() {
            return None;
        }
        // denote end of metadata
        self.0.push(0xff);

        Some(MetadataView(self))
    }
}

#[derive(Debug)]
pub struct MetadataView<'a>(&'a mut MetadataBuilder);

impl Deref for MetadataView<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0.0[..]
    }
}

impl Drop for MetadataView<'_> {
    fn drop(&mut self) {
        self.0.0.clear();
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::simulation::metadata::EntityBitFlags;
//
//     #[test]
//     fn test_metadata_tracker() {
//         let mut tracker = StateObserver(UnsafeCell::new(vec![]));
//
//         let air_supply = AirSupply {
//             ticks: 10,
//         };
//
//         // &Tracked<AirSupply>
//         let mut air_supply = Observable::new(air_supply);
//
//         let entity_bit_flags = EntityBitFlags::default()
//             .with_invisible(true)
//             .with_flying_with_elytra(true);
//
//         let mut entity_bit_flags = Observable::new(entity_bit_flags);
//
//         air_supply.observe(&mut tracker).ticks = 5;
//
//         entity_bit_flags.observe(&mut tracker)
//             .set_crouching(true);
//     }
//
//     fn modify(
//         air_supply: &mut Observable<AirSupply>, // ECS &mut Tracked<AirSupply>
//         ebs: &mut Observable<EntityBitFlags>, // ECS &mut Tracked<EntityBitFlags>
//         tracker: &StateObserver,
//     ) {
//         air_supply.observe(tracker).ticks = 5;
//         let mut ebs = ebs.observe(tracker);
//         ebs.set_sprinting(false);
//     }
// }
