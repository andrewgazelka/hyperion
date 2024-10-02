use std::borrow::Cow;

use flecs_ecs::{
    core::{EntityViewGet, QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    net::{Compose, NetworkStreamRef},
    simulation::{
        blocks::{EntityAndSequence, MinecraftWorld},
        event,
    },
    storage::EventQueue,
    system_registry::SystemId,
    valence_protocol::{packets::play, text::IntoText, BlockState, ItemStack, VarInt},
};
use tracing::trace_span;

#[derive(Component)]
pub struct BlockModule;

impl Module for BlockModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        // todo: this is a hack. We want the system ID to be automatically assigned based on the location of the system.
        let system_id = SystemId(8);

        system!("handle_blocks", world, &mut MinecraftWorld($), &mut EventQueue<event::DestroyBlock>($), &Compose($))
            .multi_threaded()
            .each_iter(move |it: TableIter<'_, false>, _, (mc, event_queue, compose)| {
                let span = trace_span!("handle_blocks");
                let _enter = span.enter();

                let world = it.world();


                for event in event_queue.drain() {
                    let Ok(previous) = mc.try_set_block_delta(event.position, BlockState::AIR) else {
                        return;
                    };

                    let from = event.from;
                    let net = world.entity_from_id(from).get::<&NetworkStreamRef>(|id| *id);

                    mc.mark_should_update(event.position);
                    mc.to_confirm.push(EntityAndSequence {
                        entity: event.from,
                        sequence: event.sequence,
                    });


                    // Create a message about the broken block
                    let msg = format!("Block broken: {:?} at {:?}", previous, event.position);

                    let pkt = play::GameMessageS2c {
                        chat: msg.into_cow_text(),
                        overlay: false,
                    };

                    // Send the message to the player
                    compose.unicast(&pkt, net, system_id, &world).unwrap();

                    // let pkt = play::InventoryS2c {
                    //     window_id: 0,
                    //     state_id: Default::default(),
                    //     slots: Default::default(),
                    //     carried_item: Default::default(),
                    // }

                    let previous = previous.to_kind().to_item_kind();

                    let pkt = play::ScreenHandlerSlotUpdateS2c {
                        window_id: 0, // the player's slot is always 0
                        state_id: VarInt(0), // todo: probably not right
                        slot_idx: 36,
                        slot_data: Cow::Owned(ItemStack::new(previous, 1, None)),
                    };

                    compose.unicast(&pkt, net, system_id, &world).unwrap();
                }
            });
    }
}
