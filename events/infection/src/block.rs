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
    valence_protocol::{
        ident,
        math::IVec3,
        packets::play,
        sound::{SoundCategory, SoundId},
        text::IntoText,
        BlockState, ItemStack, VarInt,
    },
};
use tracing::trace_span;

#[derive(Component)]
pub struct BlockModule;

impl Module for BlockModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        // todo: this is a hack. We want the system ID to be automatically assigned based on the location of the system.
        let system_id = SystemId(8);

        system!("handle_destroyed_blocks", world, &mut MinecraftWorld($), &mut EventQueue<event::DestroyBlock>($), &Compose($), &mut hyperion_inventory::PlayerInventory)
            .multi_threaded()
            .each_iter(move |it: TableIter<'_, false>, _, (mc, event_queue, compose, inventory)| {
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

                    let position = event.position;
                    let position = IVec3::new(position.x << 3, position.y << 3, position.z << 3);


                    let ident = ident!("minecraft:block.note_block.harp");
                    // Send a note sound when breaking a block
                    let pkt = play::PlaySoundS2c {
                        id: SoundId::Direct { id: ident.into(), range: None },
                        position,
                        volume: 1.0,
                        pitch: 1.0,
                        seed: 0,
                        category: SoundCategory::Block,
                    };
                    compose.unicast(&pkt, net, system_id, &world).unwrap();


                    let diff = ItemStack::new(previous.to_kind().to_item_kind(), 1, None);

                    let added_slots = inventory.try_add_item(diff);

                    for slot in added_slots.changed_slots {
                        let item = inventory.get(slot).unwrap();

                        let pkt = play::ScreenHandlerSlotUpdateS2c {
                            window_id: 0, // the player's slot is always 0
                            state_id: VarInt(0), // todo: probably not right
                            slot_idx: slot as i16,
                            slot_data: Cow::Borrowed(item),
                        };
                        compose.unicast(&pkt, net, system_id, &world).unwrap();
                    }

                    compose.unicast(&pkt, net, system_id, &world).unwrap();
                }
            });

        system!("handle_placed_blocks", world, &mut MinecraftWorld($), &mut EventQueue<event::PlaceBlock>($))
            .multi_threaded()
            .each_iter(move |_it: TableIter<'_, false>, _, (mc, event_queue)| {
                let span = trace_span!("handle_placed_blocks");
                let _enter = span.enter();
                for event in event_queue.drain() {
                    let position = event.position;

                    mc.try_set_block_delta(position, event.block).unwrap();

                    mc.to_confirm.push(EntityAndSequence {
                        entity: event.from,
                        sequence: event.sequence,
                    });
                }
            }
            );
    }
}
