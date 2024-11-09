use clap::Parser;
use flecs_ecs::core::{Entity, EntityViewGet, World, WorldGet};
use hyperion::net::{agnostic, Compose, DataBundle, NetworkStreamRef};
use hyperion::system_registry::SystemId;
use hyperion::valence_protocol::packets::play::player_abilities_s2c::PlayerAbilitiesFlags;
use hyperion::valence_protocol::packets::play::PlayerAbilitiesS2c;
use hyperion_clap::MinecraftCommand;

#[derive(Parser, Debug)]
#[command(name = "speed")]
pub struct SpeedCommand {
    amount: f32,
}


impl MinecraftCommand for SpeedCommand {
    fn execute(self, world: &World, caller: Entity) {
        let msg = format!("Setting speed to {}", self.amount);
        let chat = agnostic::chat(msg);

        world.get::<&Compose>(|compose| {
            caller
                .entity_view(world)
                .get::<&NetworkStreamRef>(|stream| {
                    let packet = speed_packet(self.amount);

                    let mut bundle = DataBundle::new(compose);
                    bundle.add_packet(&packet, world).unwrap();
                    bundle.add_packet(&chat, world).unwrap();

                    bundle.send(world, *stream, SystemId(8)).unwrap();
                });
        });
    }
}

fn speed_packet(amount: f32) -> PlayerAbilitiesS2c {
    PlayerAbilitiesS2c {
        flags: PlayerAbilitiesFlags::default()
            .with_flying(true)
            .with_allow_flying(true),
        flying_speed: amount,
        fov_modifier: 0.0,
    }
}
