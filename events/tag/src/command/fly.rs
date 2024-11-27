use clap::Parser;
use flecs_ecs::{
    core::{Entity, EntityViewGet, World, WorldGet},
    macros::Component,
};
use hyperion::{
    net::{Compose, DataBundle, NetworkStreamRef, agnostic},
    system_registry::SystemId,
    valence_protocol::packets::play::{
        PlayerAbilitiesS2c, player_abilities_s2c::PlayerAbilitiesFlags,
    },
};
use hyperion_clap::{CommandPermission, MinecraftCommand};

#[derive(Component)]
pub struct Flight;

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "fly")]
#[command_permission(group = "Moderator")]
pub struct FlyCommand;

impl MinecraftCommand for FlyCommand {
    fn execute(self, world: &World, caller: Entity) {
        world.get::<&Compose>(|compose| {
            let allow_flight: bool = !caller.entity_view(world).has::<Flight>();

            let chat_packet = if allow_flight {
                agnostic::chat("§aFlying enabled")
            } else {
                agnostic::chat("§cFlying disabled")
            };

            caller.entity_view(world).add_if::<Flight>(allow_flight);

            caller
                .entity_view(world)
                .get::<&NetworkStreamRef>(|stream| {
                    let packet = fly_packet(allow_flight);

                    let mut bundle = DataBundle::new(compose);
                    bundle.add_packet(&packet, world).unwrap();
                    bundle.add_packet(&chat_packet, world).unwrap();

                    bundle.send(world, *stream, SystemId(8)).unwrap();
                });
        });
    }
}

fn fly_packet(allow_flight: bool) -> PlayerAbilitiesS2c {
    const SPEED_METER_PER_SECOND: f32 = 10.92;

    // guessing.. idk what the actual conversion is
    const SOME_CONVERSION: f32 = SPEED_METER_PER_SECOND / 70.0;

    PlayerAbilitiesS2c {
        flags: PlayerAbilitiesFlags::default()
            .with_flying(allow_flight)
            .with_allow_flying(allow_flight),
        flying_speed: SOME_CONVERSION,
        fov_modifier: 0.0,
    }
}
