use clap::Parser;
use flecs_ecs::core::{Entity, EntityViewGet, World, WorldGet};
use hyperion::{
    net::{Compose, DataBundle, NetworkStreamRef},
    system_registry::SystemId,
    valence_protocol::{VarInt, packets::play},
};
use hyperion_clap::MinecraftCommand;

#[derive(Parser, Debug)]
#[command(name = "xp")]
pub struct XpCommand {
    bar: f32,
    level: i32,
}

impl MinecraftCommand for XpCommand {
    fn execute(self, world: &World, caller: Entity) {
        let Self { bar, level } = self;

        let xp_pkt = play::ExperienceBarUpdateS2c {
            bar,
            level: VarInt(level),
            total_xp: VarInt::default(),
        };

        world.get::<&Compose>(|compose| {
            caller
                .entity_view(world)
                .get::<&NetworkStreamRef>(|stream| {
                    let mut bundle = DataBundle::new(compose);
                    bundle.add_packet(&xp_pkt, world).unwrap();

                    bundle.send(world, *stream, SystemId(8)).unwrap();
                });
        });
    }
}
