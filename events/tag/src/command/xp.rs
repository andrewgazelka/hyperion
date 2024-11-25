use clap::Parser;
use flecs_ecs::core::{Entity, EntityViewGet, World};
use hyperion::simulation::Xp;
use hyperion_clap::MinecraftCommand;

#[derive(Parser, Debug)]
#[command(name = "xp")]
pub struct XpCommand {
    amount: u16,
}

impl MinecraftCommand for XpCommand {
    fn execute(self, world: &World, caller: Entity) {
        let Self { amount } = self;

        caller.entity_view(world).get::<&mut Xp>(|xp| {
            xp.amount = amount;
        });
    }
}
