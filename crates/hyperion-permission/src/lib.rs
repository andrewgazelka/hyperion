use clap::ValueEnum;
use flecs_ecs::{
    core::{
        EntityViewGet, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World, WorldGet, WorldProvider,
    },
    macros::{Component, observer},
    prelude::{Module, flecs},
};
use hyperion::{
    net::{Compose, NetworkStreamRef},
    simulation::{Uuid, command::get_command_packet},
    storage::LocalDb,
    system_registry::SystemId,
};
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

        observer!(world, flecs::OnSet, &Group).each_entity(|entity, _group| {
            let world = entity.world();

            let root_command = hyperion::simulation::command::get_root_command_entity();

            let cmd_pkt = get_command_packet(&world, root_command, Some(*entity));

            entity.get::<&NetworkStreamRef>(|stream| {
                world.get::<&Compose>(|compose| {
                    compose
                        .unicast(&cmd_pkt, *stream, SystemId(999), &world)
                        .unwrap();
                });
            });
        });
    }
}
