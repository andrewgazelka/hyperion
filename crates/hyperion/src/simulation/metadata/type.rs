use valence_protocol::VarInt;

use crate::simulation::metadata::Pose;

pub trait MetadataType {
    const INDEX: i32;
}

impl MetadataType for u8 {
    #[allow(clippy::use_self)]
    const INDEX: i32 = 0;
}

impl MetadataType for f32 {
    const INDEX: i32 = 3;
}

impl MetadataType for Pose {
    const INDEX: i32 = 20;
}

impl MetadataType for VarInt {
    const INDEX: i32 = 1;
}
