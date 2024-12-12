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
use valence_protocol::{VarInt, game_mode::OptGameMode, packets::play};
use valence_server::{GameMode, ident};

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
                    let client = event.client.entity_view(world);
                    client
                        .get::<(&ConnectionId, &mut Health)>(|(connection, health)| {
                            health.heal(20.);
                            client.set::<Pose>(Pose::Standing);

                            let pkt_health = play::HealthUpdateS2c {
                                health: health.abs(),
                                food: VarInt(20),
                                food_saturation: 5.0,
                            };

                            let pkt_respawn = play::PlayerRespawnS2c {
                                dimension_type_name: ident!("minecraft:overworld").into(),
                                dimension_name: ident!("minecraft:overworld").into(),
                                hashed_seed: 0,
                                game_mode: GameMode::Survival,
                                previous_game_mode: OptGameMode::default(),
                                is_debug: false,
                                is_flat: false,
                                copy_metadata: false,
                                last_death_location: None,
                                portal_cooldown: VarInt::default(),
                            };

                            compose.unicast(&pkt_health, *connection, system).unwrap();
                            compose.unicast(&pkt_respawn, *connection, system).unwrap();
                        });
                }
            });
    }
}
