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
    valence_protocol::{packets::play, text::IntoText},
    SystemRegistry,
};
use tracing::{debug, trace_span};

use crate::component::team::Team;

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

                let Ok((_, command)) = parse_command(executed) else {
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

                        // let pkt = play::EntityAttributesS2c {
                        //     entity_id: VarInt(0), // every player thinks they are 0
                        //     properties: vec![
                        //         AttributeProperty {
                        //             key: ident!("generic.movement_speed").into(),
                        //             value: amount,
                        //             modifiers: vec![],
                        //         },
                        //         AttributeProperty {
                        //             key: ident!("generic.flying_speed").into(),
                        //             value: amount,
                        //             modifiers: vec![],
                        //         },
                        //     ],
                        // };
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
    play::PlayerAbilitiesS2c {
        flags: PlayerAbilitiesFlags::default()
            .with_allow_flying(true)
            .with_flying(true),
        flying_speed: amount,
        fov_modifier: 0.0,
    }
}

use hyperion::valence_protocol::{
    ident,
    packets::play::{
        entity_attributes_s2c::AttributeProperty, player_abilities_s2c::PlayerAbilitiesFlags,
        PlayerAbilitiesS2c,
    },
    Encode, VarInt,
};
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{digit1, space1},
    combinator::{map, map_res},
    number::complete::float,
    sequence::preceded,
    IResult,
};

#[derive(Debug, PartialEq)]
enum ParsedCommand {
    Speed(f32),
    Team,
    Zombie,
}

fn parse_speed(input: &str) -> IResult<&str, ParsedCommand> {
    map(
        preceded(preceded(tag("speed"), space1), float),
        ParsedCommand::Speed,
    )(input)
}

fn parse_team(input: &str) -> IResult<&str, ParsedCommand> {
    map(tag("team"), |_| ParsedCommand::Team)(input)
}

fn parse_zombie(input: &str) -> IResult<&str, ParsedCommand> {
    map(tag("zombie"), |_| ParsedCommand::Zombie)(input)
}

fn parse_command(input: &str) -> IResult<&str, ParsedCommand> {
    alt((parse_speed, parse_team, parse_zombie))(input)
}
