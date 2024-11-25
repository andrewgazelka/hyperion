use clap::Parser;
use flecs_ecs::core::{Entity, EntityViewGet, World, WorldGet};
use hyperion::{
    net::{Compose, DataBundle, NetworkStreamRef, agnostic},
    system_registry::SystemId,
    valence_protocol::packets::play::{
        PlayerAbilitiesS2c, player_abilities_s2c::PlayerAbilitiesFlags,
    },
};
use hyperion_clap::MinecraftCommand;

#[derive(Parser, Debug)]
#[command(name = "fly")]
pub struct FlyCommand;

impl MinecraftCommand for FlyCommand {
    fn execute(self, world: &World, caller: Entity) {
        let chat = agnostic::chat("§aFlying enabled");

        world.get::<&Compose>(|compose| {
            caller
                .entity_view(world)
                .get::<&NetworkStreamRef>(|stream| {
                    let packet = fly_packet();

                    let mut bundle = DataBundle::new(compose);
                    bundle.add_packet(&packet, world).unwrap();
                    bundle.add_packet(&chat, world).unwrap();

                    bundle.send(world, *stream, SystemId(8)).unwrap();
                });
        });
    }
}

fn fly_packet() -> PlayerAbilitiesS2c {
    const SPEED_METER_PER_SECOND: f32 = 10.92;

    // guessing.. idk what the actual conversion is
    const SOME_CONVERSION: f32 = SPEED_METER_PER_SECOND / 70.0;

    PlayerAbilitiesS2c {
        flags: PlayerAbilitiesFlags::default()
            .with_flying(true)
            .with_allow_flying(true),
        flying_speed: SOME_CONVERSION,
        fov_modifier: 0.0,
    }
}
