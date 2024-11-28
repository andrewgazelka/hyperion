use flecs_ecs::prelude::*;
use valence_generated::block::BlockState;

use super::Metadata;
use crate::define_and_register_components;

// Example usage:
define_and_register_components! {
    23, DisplayedBlockState -> BlockState,
}
