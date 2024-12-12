use hyperion::{
    flecs_ecs::{
        self,
        core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World},
        macros::{system, Component},
        prelude::Module,
    },
    net::{Compose, ConnectionId},
    protocol::{game_mode::OptGameMode, packets::play, VarInt},
    server::{ident, GameMode},
    simulation::{
        event::ClientStatusEvent,
        metadata::{entity::Pose, living_entity::Health},
    },
    storage::EventQueue,
};

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
                    client.get::<(&ConnectionId, &mut Health, &mut Pose)>(
                        |(connection, health, pose)| {
                            health.heal(20.);

                            *pose = Pose::Standing;
                            client.modified::<Pose>(); // this is so observers detect the change

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
                        },
                    );
                }
            });
    }
}
