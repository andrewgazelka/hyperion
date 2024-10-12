use flecs_ecs::{
    core::{flecs, EntityViewGet, QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    net::Compose,
    simulation::{event, metadata::Metadata, EntityReaction, Health, Player, Position},
    storage::EventQueue,
    system_registry::SystemId,
    valence_protocol::{
        ident,
        packets::play,
        sound::{SoundCategory, SoundId},
        VarInt,
    },
};
use tracing::trace_span;

#[derive(Component)]
pub struct LevelModule;

#[derive(Component, Default, Copy, Clone, Debug)]
pub struct Level {
    pub value: usize,
}

impl Module for LevelModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        world
            .component::<Player>()
            .add_trait::<(flecs::With, Level)>(); // todo: how does this even call Default? (IndraDb)
    }
}
