use flecs_ecs::{
    core::{flecs, World},
    macros::Component,
    prelude::Module,
};
use hyperion::simulation::Player;

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
