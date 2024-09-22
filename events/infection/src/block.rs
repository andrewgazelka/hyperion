use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    simulation::{
        blocks::{EntityAndSequence, MinecraftWorld},
        event,
    },
    storage::EventQueue,
    valence_protocol::BlockState,
};
use tracing::trace_span;

#[derive(Component)]
pub struct BlockModule;

impl Module for BlockModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        system!("handle_blocks", world, &mut MinecraftWorld($), &mut EventQueue<event::DestroyBlock>($))
            .multi_threaded()
            .each_iter(move |_it: TableIter<'_, false>, _, (mc, event_queue)| {
                let span = trace_span!("handle_blocks");
                let _enter = span.enter();

                for event in event_queue.drain() {
                    mc.try_set_block_delta(event.position, BlockState::AIR);
                    mc.mark_should_update(event.position);
                    mc.to_confirm.push(EntityAndSequence {
                        entity: event.from,
                        sequence: event.sequence,
                    });
                }
            });
    }
}
