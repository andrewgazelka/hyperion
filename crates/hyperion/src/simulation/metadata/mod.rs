use std::ops::Deref;

use flecs_ecs::macros::Component;
use valence_protocol::{Encode, VarInt};

#[derive(Component, Debug, Default)]
// index (u8), type (varint), value (varies)
pub struct Metadata(Vec<u8>);

mod status;

// status flags = u8

mod kind {
    use valence_protocol::VarInt;

    pub const BYTE: VarInt = VarInt(0);
    pub const POSE: VarInt = VarInt(20);
}

#[derive(Encode)]
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

impl Metadata {
    fn index(&mut self, index: u8) {
        self.0.push(index);
    }

    fn kind(&mut self, kind: VarInt) {
        kind.encode(&mut self.0).unwrap();
    }

    pub fn status(&mut self, status: status::EntityStatus) {
        self.index(0);
        self.kind(kind::BYTE);
        self.0.push(status.0);
    }

    pub fn pose(&mut self, pose: Pose) {
        self.index(6);
        self.kind(kind::POSE);
        pose.encode(&mut self.0).unwrap();
    }

    pub fn get_and_clear(&mut self) -> Option<MetadataView<'_>> {
        if self.0.is_empty() {
            return None;
        }
        // denote end of metadata
        self.0.push(0xff);
        Some(MetadataView(self))
    }
}

#[derive(Debug)]
pub struct MetadataView<'a>(&'a mut Metadata);

impl Deref for MetadataView<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0 .0
    }
}

impl Drop for MetadataView<'_> {
    fn drop(&mut self) {
        self.0 .0.clear();
    }
}
