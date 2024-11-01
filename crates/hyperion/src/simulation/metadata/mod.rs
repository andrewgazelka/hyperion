use std::{cell::UnsafeCell, ops::Deref};

use bitfield_struct::bitfield;
use flecs_ecs::macros::Component;
use valence_protocol::{Encode, VarInt};

use crate::simulation::metadata::r#type::MetadataType;

pub mod tracked;

#[derive(Component, Debug, Default)]
// index (u8), type (varint), value (varies)
/// <https://wiki.vg/Entity_metadata>
///
/// Tracks updates within a gametick for the metadata
pub struct StateObserver(UnsafeCell<Vec<u8>>);

unsafe impl Send for StateObserver {}

// technically not Sync but I mean do we really care? todo: Indra
unsafe impl Sync for StateObserver {}

mod status;

mod r#type;

pub trait Metadata {
    const INDEX: u8;
    type Type: MetadataType + Encode;
    fn to_type(self) -> Self::Type;
}

// Entity flags using bitfield
#[bitfield(u8)]
#[derive(Component)]
pub struct EntityBitFlags {
    pub on_fire: bool,   // 0x01
    pub crouching: bool, // 0x02
    #[skip]
    __: bool, // 0x04 (unused/previously riding)
    pub sprinting: bool, // 0x08
    pub swimming: bool,  // 0x10
    pub invisible: bool, // 0x20
    pub glowing: bool,   // 0x40
    pub flying_with_elytra: bool, // 0x80
}

impl Metadata for EntityBitFlags {
    type Type = u8;

    const INDEX: u8 = 0;

    fn to_type(self) -> Self::Type {
        self.0
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

#[derive(Encode, Clone, Copy)]
#[repr(u8)]
#[derive(Component)]
pub enum Pose {
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

impl StateObserver {
    pub fn is_empty(&self) -> bool {
        let inner = unsafe { &mut *self.0.get() };
        inner.is_empty()
    }

    pub fn append<M: Metadata>(&self, metadata: M) {
        let inner = unsafe { &mut *self.0.get() };

        let value_index = M::INDEX;

        inner.push(value_index);
        let type_index = VarInt(<M as Metadata>::Type::INDEX);

        type_index.encode(&mut *inner).unwrap();

        let r#type = metadata.to_type();
        r#type.encode(inner).unwrap();
    }

    pub fn get_and_clear(&mut self) -> Option<MetadataView<'_>> {
        if self.is_empty() {
            return None;
        }
        // denote end of metadata

        let inner = unsafe { &mut *self.0.get() };
        inner.push(0xff);

        Some(MetadataView(self))
    }
}

#[derive(Debug)]
pub struct MetadataView<'a>(&'a mut StateObserver);

impl Deref for MetadataView<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        let inner = unsafe { &mut *self.0 .0.get() };
        &inner[..]
    }
}

impl Drop for MetadataView<'_> {
    fn drop(&mut self) {
        let inner = unsafe { &mut *self.0 .0.get() };
        inner.clear();
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
