use flecs_ecs::{
    core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World},
    macros::{Component, system},
    prelude::Module,
};
use hyperion::{
    net::{Compose, ConnectionId},
    simulation::{
        event::ClientStatusEvent,
        metadata::{entity::Pose, living_entity::Health},
    },
    storage::EventQueue,
};
use valence_protocol::{VarInt, packets::play};

#[derive(Component)]
pub struct RespawnModule;

impl Module for RespawnModule {
    fn module(world: &World) {
        system!("handle_respawn", world, &mut EventQueue<ClientStatusEvent>($),  &Compose($))
            .multi_threaded()
            .each_iter(|it, _, (event_queue, compose)| {
                let world = it.world();
                let system = it.system();
                for event in event_queue.drain() {
                    event
                        .client
                        .entity_view(world)
                        .get::<(&ConnectionId, &mut Health)>(|(connection, health)| {
                            let player = event.client.entity_view(world);
                            health.heal(20.);
                            player.set::<Pose>(Pose::Standing);

                            let pkt_health = play::HealthUpdateS2c {
                                health: health.abs(),
                                food: VarInt(20),
                                food_saturation: 5.0,
                            };

                            compose.unicast(&pkt_health, *connection, system).unwrap();
                        });
                }
            });
    }
}
