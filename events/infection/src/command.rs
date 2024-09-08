use flecs_ecs::{
    core::{QueryBuilderImpl, TermBuilderImpl, World, WorldProvider},
    macros::system,
};
use hyperion::{
    component::command::{get_root_command, Command},
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
    let _team_command = world
        .entity()
        .set(Command::literal("team"))
        .child_of_id(root_command);
}

pub fn process(world: &World, registry: &mut SystemRegistry) {
    let system_id = registry.register();

    system!(
        "handle_infection_events_player",
        world,
        &Compose($),
        &mut EventQueue,
        &NetworkStreamRef,
        ?&Team,
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

                if let Some(team) = team
                    && executed == "team"
                {
                    let msg = format!("You are now in team {team}");

                    let text = play::GameMessageS2c {
                        chat: msg.into_cow_text(),
                        overlay: false,
                    };

                    compose.unicast(&text, *stream, system_id, &world).unwrap();
                }

                // send messages with
            });

            iterator.run(event_queue);
        },
    );
}
