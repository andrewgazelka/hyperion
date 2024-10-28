use flecs_ecs::{
    core::{flecs, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World, WorldGet},
    macros::Component,
    prelude::Module,
};
use valence_protocol::packets::play::command_tree_s2c::{Parser, StringArg};
use hyperion::{commands, simulation::{
    command::{add_command, get_root_command, Command},
    Player,
}, storage::LocalDb};
use hyperion::simulation::command::CommandDsl;

#[derive(Component)]
pub struct PermissionModule;

mod storage;

impl Module for PermissionModule {
    fn module(world: &World) {
        use hyperion::commands_inner;

        commands!(world => {
            "give" {
                "player": Parser::Entity {
                    "item" : Parser::ItemStack
                }
            }
            "gamemode" {
                "mode": Parser::GameMode
            }
            "help"
        });


        // based on luckperms https://luckperms.net/wiki/Command-Usage
        // https://luckperms.net/wiki/General-Commands
        // let perms = add_command(world, Command::literal("perms"), root_command);
        // add_command()


        world.get::<&LocalDb>(|db| {
            let storage = storage::PermissionStorage::new(db).unwrap();
            world.set(storage);
        });
    }
}
