use flecs_ecs::{
    core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    simulation::{event, InGameName, Position},
    storage::EventQueue,
    system_registry::SystemId,
    valence_protocol::{packets::play, text::IntoText},
};

#[derive(Component)]
pub struct ChatModule;

impl Module for ChatModule {
    fn module(world: &World) {
        let system_id = SystemId(8);

        system!("handle_chat_messages", world, &mut EventQueue<event::ChatMessage<'static>>($), &hyperion::net::Compose($))
            .multi_threaded()
            .each_iter(move |it: TableIter<'_, false>, _: usize, (event_queue, compose): (&mut EventQueue<event::ChatMessage<'static>>, &hyperion::net::Compose)| {
                let world = it.world();

                for event::ChatMessage { msg, by } in event_queue.drain() {
                    let by = world.entity_from_id(by);

                    // todo: we should not need this; death should occur such that this is always valid
                    if !by.is_alive() {
                        continue;
                    }

                    // todo: try_get if entity is dead/not found what will happen?
                    by.get::<(&InGameName, &Position)>(|(name, position)| {
                        let chat = format!("§8<§b{name}§8>§r {msg}").into_cow_text();
                        let packet = play::GameMessageS2c {
                            chat,
                            overlay: false,
                        };

                        let center = position.chunk_pos();

                        compose.broadcast_local(&packet, center, system_id)
                            .send(&world)
                            .unwrap();
                    });
                }
            });
    }
}
