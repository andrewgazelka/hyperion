use flecs_ecs::core::{World, WorldGet};
use flecs_ecs::macros::Component;
use flecs_ecs::prelude::Module;
use hyperion::runtime::AsyncRuntime;

#[derive(Component)]
pub struct GenMapModule;

impl Module for GenMapModule {
    fn module(world: &World) {
        world.get::<&AsyncRuntime>(|runtime| {
            let f = hyperion_utils::cached_save(
                world,
                "https://github.com/andrewgazelka/maps/raw/main/GenMap.tar.gz",
            );

            runtime.schedule(f, |result, world| {
                let save = result.unwrap();
                world.set(Blocks::new(world, &save).unwrap());
            });
        });

    }
}

