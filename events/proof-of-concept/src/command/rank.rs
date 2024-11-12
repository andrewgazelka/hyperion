use std::borrow::Cow;
use clap::Parser;
use flecs_ecs::core::{Entity, EntityViewGet, World, WorldGet};
use hyperion::{
    net::{Compose, DataBundle, NetworkStreamRef, agnostic},
    system_registry::SystemId,
};
use hyperion::valence_protocol::packets::play;
use hyperion::valence_protocol::VarInt;
use hyperion_clap::MinecraftCommand;

#[derive(Parser, Debug)]
#[command(name = "rank")]
pub struct RankCommand {
    rank: hyperion_rank_tree::Rank,
}

impl MinecraftCommand for RankCommand {
    fn execute(self, world: &World, caller: Entity) {
        let rank = self.rank;
        let msg = format!("Setting rank to {rank:?}");
        let chat = agnostic::chat(msg);
        
        let inv = rank.inventory();
        
        let slots = inv.slots();
        
        let inv_pkt = play::InventoryS2c {
            window_id: 0,
            state_id: VarInt(0),
            slots: Cow::Borrowed(slots),
            carried_item: Cow::default(),
        };

        world.get::<&Compose>(|compose| {
            caller
                .entity_view(world)
                .get::<&NetworkStreamRef>(|stream| {
                    let mut bundle = DataBundle::new(compose);
                    bundle.add_packet(&chat, world).unwrap();
                    bundle.add_packet(&inv_pkt, world).unwrap();

                    bundle.send(world, *stream, SystemId(8)).unwrap();
                });
        });
    }
}
