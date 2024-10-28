use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TermBuilderImpl, World, WorldGet},
    macros::{observer, system, Component},
    prelude::{flecs, Module, TableIter},
};
use hyperion::{
    net::Compose,
    simulation::{command::cmd_with, event, Uuid},
    storage::{EventQueue, LocalDb},
};
use num_derive::{FromPrimitive, ToPrimitive};
use valence_protocol::packets::play::command_tree_s2c::Parser;

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

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{alpha1, space0},
    combinator::{map, value},
    sequence::{preceded, tuple},
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
        preceded((tag("set"), space0), alpha1),
        preceded(space0, parse_group),
    )
        .parse(input)
}

#[derive(Debug, Eq, PartialEq)]
pub enum Command<'a> {
    Set { player: &'a str, group: Group },
    Get { player: &'a str },
}

fn parse_get_command(input: &str) -> IResult<&str, &str> {
    preceded((tag("get"), space0), alpha1).parse(input)
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

impl Module for PermissionModule {
    fn module(world: &World) {
        cmd_with(world, "perms", |scope| {
            scope.literal_with("set", |scope| {
                scope.argument("player", Parser::Entity {
                    single: true,
                    only_players: true,
                });
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

        observer!(world, flecs::OnSet, &Uuid, &storage::PermissionStorage($)).each(
            |(uuid, permissions)| {
                let res = permissions.get(**uuid);
            },
        );

        system!("perms_command", world, &Compose($), &mut EventQueue<event::Command>($))
            .kind::<flecs::pipeline::OnUpdate>()
            .each_iter(move |it: TableIter<'_, false>, _, (compose, queue)| {
                let world = it.world();

                for command in queue.drain() {
                    let by = command.by;

                    let Ok((assert_none, command)) = parse_command(command.raw) else {
                        continue;
                    };

                    match command {
                        Command::Set { player, group } => {}
                        Command::Get { player } => {}
                    }
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
