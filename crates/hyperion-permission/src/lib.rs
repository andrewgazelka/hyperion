use clap::ValueEnum;
use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TermBuilderImpl, World, WorldGet},
    macros::{Component, observer},
    prelude::{Module, flecs},
};
use hyperion::{simulation::Uuid, storage::LocalDb};
use num_derive::{FromPrimitive, ToPrimitive};

#[derive(Component)]
pub struct PermissionModule;

mod storage;

#[derive(
    Default,
    Component,
    FromPrimitive,
    ToPrimitive,
    Copy,
    Clone,
    Debug,
    PartialEq,
    ValueEnum,
    Eq
)]
#[repr(C)]
pub enum Group {
    Banned,
    #[default]
    Normal,
    Moderator,
    Admin,
}

#[derive(clap::Parser, Debug)]
pub struct SetCommand {
    player: String,
    group: Group,
}

#[derive(clap::Parser, Debug)]
pub struct GetCommand {
    player: String,
}

#[derive(clap::Parser, Debug)]
#[command(name = "perms")]
pub enum PermissionCommand {
    Set(SetCommand),
    Get(GetCommand),
}

// todo:

impl Module for PermissionModule {
    fn module(world: &World) {
        world.component::<Group>();
        world.component::<storage::PermissionStorage>();

        world.get::<&LocalDb>(|db| {
            let storage = storage::PermissionStorage::new(db).unwrap();
            world.set(storage);
        });

        observer!(world, flecs::OnSet, &Uuid, &storage::PermissionStorage($)).each_entity(
            |entity, (uuid, permissions)| {
                let group = permissions.get(**uuid);
                entity.set(group);
            },
        );

        observer!(world, flecs::OnRemove, &Uuid, &Group, &storage::PermissionStorage($)).each(
            |(uuid, group, permissions)| {
                permissions.set(**uuid, *group).unwrap();
            },
        );
    }
}
