use valence_protocol::VarInt;

use crate::EntityType;

impl From<EntityType> for VarInt {
    fn from(value: EntityType) -> Self {
        let value = value as i32;
        Self(value)
    }
}
