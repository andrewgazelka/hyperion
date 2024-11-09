use clap::{Arg as ClapArg, Parser, ValueEnum, ValueHint, error::ErrorKind};
use flecs_ecs::{
    core::{Entity, EntityViewGet, World, WorldGet},
    prelude::{Component, Module},
};
use hyperion::{
    net::{Compose, agnostic},
    simulation::command::get_root_command_entity,
    system_registry::SystemId,
};
pub use hyperion_command;
use hyperion_command::CommandRegistry;
use valence_protocol::packets::play::command_tree_s2c::StringArg;

pub trait MinecraftCommand: Parser {
    fn execute(self, world: &World, caller: Entity);

    fn register(registry: &mut CommandRegistry, world: &World) {
        let cmd = Self::command();
        let name = cmd.get_name();

        let node_to_register = hyperion::simulation::command::Command::literal(name);

        let mut on = world
            .entity()
            .set(node_to_register)
            .child_of_id(get_root_command_entity());

        for arg in cmd.get_arguments() {
            use valence_protocol::packets::play::command_tree_s2c::Parser as ValenceParser;
            let name = arg.get_value_names().unwrap().first().unwrap();
            let name = name.to_ascii_lowercase();
            let node_to_register = hyperion::simulation::command::Command::argument(
                name,
                ValenceParser::String(StringArg::SingleWord),
            );

            on = world.entity().set(node_to_register).child_of_id(on);
        }

        let to_register = |input: &str, world: &World, caller: Entity| {
            let input = input.split_whitespace();

            match Self::try_parse_from(input) {
                Ok(elem) => elem.execute(world, caller),
                Err(e) => {
                    // add red if not display help
                    let prefix = match e.kind() {
                        ErrorKind::DisplayHelp => "",
                        _ => "§c",
                    };

                    // minecraft red
                    let msg = format!("{prefix}{e}");

                    world.get::<&Compose>(|compose| {
                        caller
                            .entity_view(world)
                            .get::<&hyperion::net::NetworkStreamRef>(|stream| {
                                let msg = agnostic::chat(msg);
                                compose.unicast(&msg, *stream, SystemId(8), world).unwrap();
                            });
                    });

                    tracing::warn!("could not parse command {e}");
                }
            };
        };

        tracing::info!("registering command {name}");

        registry.register(name, to_register);
    }
}

pub enum Arg {
    Player,
}

// Custom trait for Minecraft-specific argument behavior
pub trait MinecraftArg {
    #[must_use]
    fn minecraft(self, parser: Arg) -> Self;
}

// Implement the trait for Arg
impl MinecraftArg for ClapArg {
    fn minecraft(self, arg: Arg) -> Self {
        match arg {
            Arg::Player => self.value_hint(ValueHint::Username),
        }
    }
}

#[derive(Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}

#[derive(Component)]
pub struct ClapCommandModule;

impl Module for ClapCommandModule {
    fn module(world: &World) {
        world.import::<hyperion_command::CommandModule>();
    }
}
