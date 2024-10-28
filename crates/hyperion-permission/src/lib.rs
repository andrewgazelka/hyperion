use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TermBuilderImpl, World, WorldGet},
    macros::Component,
    prelude::Module,
};
use hyperion::{simulation::command::cmd_with, storage::LocalDb};
use valence_protocol::packets::play::command_tree_s2c::Parser;

#[derive(Component)]
pub struct PermissionModule;

mod storage;

impl Module for PermissionModule {
    fn module(world: &World) {
        cmd_with(world, "perms", |scope| {
            scope.argument("player", Parser::Entity {
                single: true,
                only_players: true,
            });

            scope.literal("verbose");
        });

        // based on luckperms https://luckperms.net/wiki/Command-Usage
        // https://luckperms.net/wiki/General-Commands
        // let perms = add_command(world, Command::literal("perms"), root_command);
        // add_command()

        // world.get::<&LocalDb>(|db| {
        //     let storage = storage::PermissionStorage::new(db).unwrap();
        //     world.set(storage);
        // });
    }
}
