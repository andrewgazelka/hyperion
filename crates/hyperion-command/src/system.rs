use std::{fmt::Write, sync::OnceLock};

use flecs_ecs::{
    core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World, WorldGet},
    macros::{Component, system},
    prelude::Module,
};
use hyperion::{
    net::agnostic,
    simulation::event,
    storage::{EventQueue, GlobalEventHandlers},
    system_registry::SystemId,
};
use regex::Regex;

use crate::component::CommandRegistry;

#[derive(Component)]
pub struct CommandSystemModule;

impl Module for CommandSystemModule {
    fn module(world: &World) {
        static COMMAND_REGEX: OnceLock<Regex> = OnceLock::new();

        system!(
            "execute_command",
            world,
            &mut EventQueue<event::Command<'_>>($),
            &CommandRegistry($)
        )
        .each_iter(|it, _, (event_queue, registry)| {
            let world = it.world();
            for event::Command { raw, by } in event_queue.drain() {
                let Some(first_word) = raw.split_whitespace().next() else {
                    tracing::warn!("command is empty");
                    continue;
                };

                let Some(command) = registry.commands.get(first_word) else {
                    tracing::debug!("command {first_word} not found");

                    let mut msg = String::new();
                    write!(&mut msg, "§cAvailable commands: §r[").unwrap();

                    for w in registry.get_permitted(&world, by).intersperse(", ") {
                        write!(&mut msg, "{w}").unwrap();
                    }

                    write!(&mut msg, "]").unwrap();

                    let chat = agnostic::chat(msg);

                    world.get::<&hyperion::net::Compose>(|compose| {
                        by.entity_view(world)
                            .get::<&hyperion::net::NetworkStreamRef>(|stream| {
                                compose
                                    .unicast(&chat, *stream, SystemId(8), &world)
                                    .unwrap();
                            });
                    });

                    continue;
                };

                tracing::debug!("executing command {first_word}");

                let command = command.on_execute;
                command(raw, &world, by);
            }
        });

        let command_regex = Regex::new(r"/(?P<command>\w+).*").unwrap();
        COMMAND_REGEX.set(command_regex).unwrap();

        world.get::<&mut GlobalEventHandlers>(|handlers| {
            handlers.completion.register(|query, completion| {
                let input = completion.query;

                // should be in form "/{command}"
                let command_regex = COMMAND_REGEX.get().unwrap();
                let Some(captures) = command_regex.captures(input) else {
                    return;
                };

                let Some(command) = captures.name("command") else {
                    return;
                };

                let command = command.as_str();

                query.world.get::<&CommandRegistry>(|registry| {
                    let Some(cmd) = registry.commands.get(command) else {
                        return;
                    };
                    let on_tab = cmd.on_tab_complete;
                    on_tab(query, completion);
                });
            });
        });
    }
}
