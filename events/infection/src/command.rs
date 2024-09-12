use flecs_ecs::core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World};
use hyperion::{
    component::command::{get_root_command, Command, Parser},
    event,
    event::EventQueue,
    net::{Compose, NetworkStreamRef},
    valence_protocol::{
        packets::{
            play,
            play::{player_abilities_s2c::PlayerAbilitiesFlags, PlayerAbilitiesS2c},
        },
        text::IntoText,
    },
    SystemRegistry,
};
use tracing::{debug, trace_span};

use crate::{command::parse::ParsedCommand, component::team::Team};

mod parse;

pub fn add_to_tree(world: &World) {
    let root_command = get_root_command();

    // add to tree
    world
        .entity()
        .set(Command::literal("team"))
        .child_of_id(root_command);

    world
        .entity()
        .set(Command::literal("zombie"))
        .child_of_id(root_command);

    let speed = world
        .entity()
        .set(Command::literal("speed"))
        .child_of_id(root_command);

    world
        .entity()
        .set(Command::argument("amount", Parser::Float {
            min: Some(0.0),
            max: Some(1024.0),
        }))
        .child_of_id(speed);
}

pub fn process(world: &World, registry: &mut SystemRegistry) {
    let system_id = registry.register();

    world
        .system_named::<(&Compose, &mut EventQueue<event::Command>)>(
            "handle_infection_events_player",
        )
        .term_at(0)
        .singleton()
        .term_at(1)
        .singleton()
        .multi_threaded()
        .each_iter(move |it, _, (compose, event_queue)| {
            let span = trace_span!("handle_infection_events_player");
            let _enter = span.enter();

            let world = it.world();
            for event in event_queue.drain() {
                let executed = event.raw.as_str();

                debug!("executed: {executed}");

                let Ok((_, command)) = parse::command(executed) else {
                    return;
                };

                world
                    .entity_from_id(event.by)
                    .get::<(&NetworkStreamRef, &mut Team)>(|(stream, team)| {
                        match command {
                            ParsedCommand::Speed(amount) => {
                                let msg = format!("Setting speed to {amount}");
                                let pkt = play::GameMessageS2c {
                                    chat: msg.into_cow_text(),
                                    overlay: false,
                                };

                                compose.unicast(&pkt, *stream, system_id, &world).unwrap();

                                let pkt = fly_speed_packet(amount);
                                compose.unicast(&pkt, *stream, system_id, &world).unwrap();
                            }
                            ParsedCommand::Team => {
                                let msg = format!("You are now on team {team}");
                                let text = play::GameMessageS2c {
                                    chat: msg.into_cow_text(),
                                    overlay: false,
                                };
                                compose.unicast(&text, *stream, system_id, &world).unwrap();
                            }
                            ParsedCommand::Zombie => {
                                let msg = "Turning to zombie";

                                // todo: maybe this should be an event?
                                let text = play::GameMessageS2c {
                                    chat: msg.into_cow_text(),
                                    overlay: false,
                                };
                                compose.unicast(&text, *stream, system_id, &world).unwrap();
                                *team = Team::Zombie;
                            }
                        }
                    });
            }
        });
}

fn fly_speed_packet(amount: f32) -> PlayerAbilitiesS2c {
    PlayerAbilitiesS2c {
        flags: PlayerAbilitiesFlags::default()
            .with_allow_flying(true)
            .with_flying(true),
        flying_speed: amount,
        fov_modifier: 0.0,
    }
}
