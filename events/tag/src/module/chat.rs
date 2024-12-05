use flecs_ecs::{
    core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World, flecs},
    macros::{Component, system},
    prelude::Module,
};
use hyperion::{
    net::ConnectionId,
    simulation::{Name, Player, Position, event},
    storage::EventQueue,
    system_registry::SystemId,
    valence_protocol::{packets::play, text::IntoText},
};
use tracing::info_span;

const CHAT_COOLDOWN_SECONDS: i64 = 15; // 15 seconds
const CHAT_COOLDOWN_TICKS: i64 = CHAT_COOLDOWN_SECONDS * 20; // Convert seconds to ticks

#[derive(Default, Component)]
#[meta]
pub struct ChatCooldown {
    pub expires: i64,
}

#[derive(Component)]
pub struct ChatModule;

impl Module for ChatModule {
    fn module(world: &World) {
        let system_id = SystemId(8);

        world.component::<ChatCooldown>().meta();

        world
            .component::<Player>()
            .add_trait::<(flecs::With, ChatCooldown)>();

        system!("handle_chat_messages", world, &mut EventQueue<event::ChatMessage<'static>>($), &hyperion::net::Compose($))
            .multi_threaded()
            .each_iter(move |it: TableIter<'_, false>, _: usize, (event_queue, compose): (&mut EventQueue<event::ChatMessage<'static>>, &hyperion::net::Compose)| {
                let world = it.world();
                let span = info_span!("handle_chat_messages");
                let _enter = span.enter();

                let current_tick = compose.global().tick;

                for event::ChatMessage { msg, by } in event_queue.drain() {
                    let by = world.entity_from_id(by);

                    // todo: we should not need this; death should occur such that this is always valid
                    if !by.is_alive() {
                        continue;
                    }

                    // Check cooldown
                    // todo: try_get if entity is dead/not found what will happen?
                    by.get::<(&Name, &Position, &mut ChatCooldown, &ConnectionId)>(|(name, position, cooldown, io)| {
                        // Check if player is still on cooldown
                        if cooldown.expires > current_tick {
                            let remaining_ticks = cooldown.expires - current_tick;
                            let remaining_secs = remaining_ticks as f32 / 20.0;

                            let cooldown_msg = format!("§cPlease wait {remaining_secs:.2} seconds before sending another message").into_cow_text();

                            let packet = play::GameMessageS2c {
                                chat: cooldown_msg,
                                overlay: false,
                            };

                            compose.unicast(&packet, *io, system_id, &world).unwrap();
                            return;
                        }

                        cooldown.expires = current_tick + CHAT_COOLDOWN_TICKS;

                        let chat = format!("§8<§b{name}§8>§r {msg}").into_cow_text();
                        let packet = play::GameMessageS2c {
                            chat,
                            overlay: false,
                        };

                        let center = position.to_chunk();

                        compose.broadcast_local(&packet, center, system_id)
                            .send(&world)
                            .unwrap();
                    });
                }
            });
    }
}
