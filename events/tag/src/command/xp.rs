use clap::Parser;
use flecs_ecs::core::{Entity, EntityViewGet, World};
use hyperion::simulation::Xp;
use hyperion_clap::{CommandPermission, MinecraftCommand};

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "xp")]
#[command_permission(group = "Admin")]
pub struct XpCommand {
    amount: u16,
}

impl MinecraftCommand for XpCommand {
    fn execute(self, world: &World, caller: Entity) {
        let Self { amount } = self;

        let caller = caller.entity_view(world);

        caller.get::<&mut Xp>(|xp| {
            xp.amount = amount;
            caller.modified::<Xp>();
        });
    }
}
