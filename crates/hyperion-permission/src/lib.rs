use flecs_ecs::{
    core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World, WorldGet},
    macros::{observer, system, Component},
    prelude::{flecs, Module, TableIter},
};
use hyperion::{
    chat,
    net::Compose,
    simulation::{command::cmd_with, event, Uuid},
    storage::{EventQueue, LocalDb},
};
use num_derive::{FromPrimitive, ToPrimitive};
use valence_protocol::packets::play::command_tree_s2c::{Parser, StringArg};

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

use hyperion::{net::NetworkStreamRef, simulation::IgnMap, system_registry::SystemId};
use nom::{
    branch::alt,
    bytes::complete::{is_a, tag},
    character::complete::space0,
    combinator::{map, value},
    sequence::preceded,
    IResult, Parser as NomParser,
};

fn parse_group(input: &str) -> IResult<&str, Group> {
    let banned = value(Group::Banned, tag("banned"));
    let normal = value(Group::Normal, tag("normal"));
    let moderator = value(Group::Moderator, tag("moderator"));
    let admin = value(Group::Admin, tag("admin"));

    alt((banned, normal, moderator, admin)).parse(input)
}

fn parse_set_command(input: &str) -> IResult<&str, (&str, Group)> {
    (
        preceded(
            (tag("set"), space0),
            is_a("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"),
        ),
        preceded(space0, parse_group),
    )
        .parse(input)
}

fn parse_get_command(input: &str) -> IResult<&str, &str> {
    preceded(
        (tag("get"), space0),
        is_a("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"),
    )
    .parse(input)
}

#[derive(Debug, Eq, PartialEq)]
pub enum Command<'a> {
    Set { player: &'a str, group: Group },
    Get { player: &'a str },
}

pub fn parse_command(input: &str) -> IResult<&str, Command<'_>> {
    alt((
        map(parse_set_command, |(player, group)| Command::Set {
            player,
            group,
        }),
        map(parse_get_command, |player| Command::Get { player }),
    ))
    .parse(input)
}

pub fn parse_perms_command(input: &str) -> IResult<&str, Command<'_>> {
    preceded(tag("perms "), parse_command).parse(input)
}

impl Module for PermissionModule {
    fn module(world: &World) {
        cmd_with(world, "perms", |scope| {
            scope.literal_with("set", |scope| {
                scope.argument_with(
                    "player",
                    Parser::Entity {
                        single: true,
                        only_players: true,
                    },
                    |scope| {
                        scope.argument("group", Parser::String(StringArg::SingleWord));
                    },
                );
            });

            scope.literal_with("get", |scope| {
                scope.argument("player", Parser::Entity {
                    single: true,
                    only_players: true,
                });
            });
        });

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

        system!("perms_command", world, &Compose($), &mut EventQueue<event::Command<'_>>($), &IgnMap($))
            .kind::<flecs::pipeline::OnUpdate>()
            .each_iter(move |it: TableIter<'_, false>, _, (compose, queue, ign_map)| {
                let world = it.world();

                for command in queue.drain() {
                    let by = command.by;

                    // todo: assert as none probably
                    let Ok((_assert_none, command)) = parse_perms_command(command.raw) else {
                        println!("not parsed");
                        continue;
                    };

                    println!("parsed");

                    world.entity_from_id(by)
                        .get::<&NetworkStreamRef>(|io| {
                            match command {
                                Command::Set { player, group } => {
                                    let Some(result) = ign_map.get(player) else {
                                        let chat = chat!("Player {player} does not exist");
                                        compose.unicast(&chat, *io, SystemId(8), &world).unwrap();
                                        return;
                                    };

                                    world.entity_from_id(*result).get::<&mut Group>(|group_ptr| {
                                        *group_ptr = group;
                                    });

                                    let chat = chat!("Set group of {player} to {group:?}");
                                    compose.unicast(&chat, *io, SystemId(8), &world).unwrap();
                                }
                                Command::Get { player } => {
                                    let Some(result) = ign_map.get(player) else {
                                        let chat = chat!("Player {player} does not exist");
                                        compose.unicast(&chat, *io, SystemId(8), &world).unwrap();
                                        return;
                                    };

                                    world.entity_from_id(*result).get::<&Group>(|group_ptr| {
                                        let chat = chat!("Group of {player} is {group_ptr:?}");
                                        compose.unicast(&chat, *io, SystemId(8), &world).unwrap();
                                    });
                                }
                            }
                        });
                }
            });

        // based on luckperms https://luckperms.net/wiki/Command-Usage
        // https://luckperms.net/wiki/General-Commands
        // let perms = add_command(world, Command::literal("perms"), root_command);
        // add_command()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_group() {
        assert_eq!(parse_group("admin"), Ok(("", Group::Admin)));
        assert_eq!(parse_group("moderator"), Ok(("", Group::Moderator)));
        assert_eq!(parse_group("normal"), Ok(("", Group::Normal)));
        assert_eq!(parse_group("banned"), Ok(("", Group::Banned)));
    }

    #[test]
    fn test_parse_set_command() {
        assert_eq!(
            parse_set_command("set player admin"),
            Ok(("", ("player", Group::Admin)))
        );
    }
}
