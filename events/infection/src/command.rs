use flecs_ecs::{
    core::{QueryBuilderImpl, TermBuilderImpl, World, WorldProvider},
    macros::system,
};
use hyperion::{
    component::command::{get_root_command, Command, Parser},
    event,
    event::{EventQueue, EventQueueIterator},
    net::{Compose, NetworkStreamRef},
    tracing_ext::TracingExt,
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

    // create the iterator here

    system!(
        "handle_infection_events_player",
        world,
        &Compose($),
        &mut EventQueue,
        &NetworkStreamRef,
        &mut Team,
    )
    .multi_threaded()
    .tracing_each_entity(
        trace_span!("handle_infection_events_player"),
        move |view, (compose, event_queue, stream, team)| {
            let mut iterator = EventQueueIterator::default();

            iterator.register::<event::Command>(|event| {
                let world = view.world();
                let executed = event.raw.as_str();

                debug!("executed: {executed}");

                let Ok((_, command)) = parse::command(executed) else {
                    return;
                };

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
            iterator.run(event_queue);
        },
    );
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
