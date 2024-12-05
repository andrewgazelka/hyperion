use clap::Parser;
use flecs_ecs::core::{Entity, EntityViewGet, World, WorldGet};
use hyperion::{
    net::{Compose, ConnectionId, DataBundle, agnostic},
    system_registry::SystemId,
    valence_protocol::packets::play::{
        PlayerAbilitiesS2c, player_abilities_s2c::PlayerAbilitiesFlags,
    },
};
use hyperion_clap::{CommandPermission, MinecraftCommand};

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "speed")]
#[command_permission(group = "Moderator")]
pub struct SpeedCommand {
    amount: f32,
}

impl MinecraftCommand for SpeedCommand {
    fn execute(self, world: &World, caller: Entity) {
        let msg = format!("Setting speed to {}", self.amount);
        let chat = agnostic::chat(msg);

        world.get::<&Compose>(|compose| {
            caller.entity_view(world).get::<&ConnectionId>(|stream| {
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
