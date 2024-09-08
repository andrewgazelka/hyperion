use flecs_ecs::{
    core::{
        flecs::pipeline, Entity, EntityViewGet, QueryBuilderImpl, SystemAPI, TermBuilderImpl,
        World, WorldProvider,
    },
    macros::system,
};
use tracing::{info, info_span, instrument, trace_span};
use valence_generated::block::BlockState;
use valence_protocol::{
    ident,
    packets::play,
    sound::{SoundCategory, SoundId},
    VarInt,
};

use crate::{
    component::{
        blocks::{
            chunk::{Delta, LoadedChunk, NeighborNotify, PendingChanges},
            MinecraftWorld,
        },
        ConfirmBlockSequences, Pose,
    },
    event::{BlockBreak, EventQueue, EventQueueIterator, ThreadLocalBump},
    net::{Compose, NetworkStreamRef},
    tracing_ext::TracingExt,
    SystemRegistry,
};

pub fn handle_events(world: &World, registry: &mut SystemRegistry) {
    let system_id = registry.register();

    // 444 µs
    system!(
        "handle_events_block",
        world,
        &Compose($),
        &mut EventQueue,
        &PendingChanges,
    )
    .kind::<pipeline::PostUpdate>()
    .multi_threaded()
    .tracing_each_entity(
        info_span!("handle_events_block"),
        move |entity, (compose, event_queue, pending)| {
            let world = entity.world();
            let world = &world;

            if event_queue.is_empty() {
                return;
            }

            let mut iter = EventQueueIterator::default();

            iter.register::<BlockBreak>(|block| {
                let position = block.position;

                let delta = Delta::new(
                    position.x as u8,
                    position.y,
                    position.z as u8,
                    BlockState::AIR,
                );

                pending.push(delta, world);

                info!("block break: {block:?}");

                let entity = world.entity_from_id(block.by);

                entity.get::<(&NetworkStreamRef, &Pose)>(|(&stream, pose)| {
                    let id = ident!("minecraft:block.note_block.harp");

                    let id = SoundId::Direct {
                        id: id.into(),
                        range: None,
                    };

                    let sound = play::PlaySoundS2c {
                        id,
                        category: SoundCategory::Ambient,
                        position: pose.sound_position(),
                        volume: 1.0,
                        pitch: 1.0,
                        seed: 0,
                    };

                    compose.unicast(&sound, stream, system_id, world).unwrap();
                });
            });

            iter.run(event_queue);
        },
    );

    system!(
        "handle_chunk_neighbor_notify",
        world,
        &LoadedChunk,
        &MinecraftWorld($),
        &mut NeighborNotify,
    )
    .kind::<pipeline::PostUpdate>()
    .multi_threaded()
    .tracing_each_entity(
        info_span!("handle_chunk_neighbor_notify"),
        |entity, (chunk, mc, notify)| {
            let world = entity.world();
            chunk.process_neighbor_changes(notify, mc, &world);
        },
    );

    let system_id = registry.register();

    system!(
        "handle_chunk_pending_changes",
        world,
        &mut LoadedChunk,
        &Compose($),
        &MinecraftWorld($),
        &mut PendingChanges,
        &NeighborNotify,
    )
    .kind::<pipeline::PostUpdate>()
    .multi_threaded()
    .tracing_each_entity(
        info_span!("handle_chunk_pending_changes"),
        move |entity, (chunk, compose, mc, pending, notify)| {
            let world = entity.world();
            chunk.process_pending_changes(pending, compose, notify, mc, system_id, &world);
        },
    );

    let system_id = registry.register();

    system!(
        "confirm_block_sequences",
        world,
        &Compose($),
        &mut NetworkStreamRef,
        &mut ConfirmBlockSequences,
    )
    .kind::<pipeline::PostUpdate>()
    .multi_threaded()
    .tracing_each_entity(
        info_span!("confirm_block_sequences"),
        move |entity, (compose, &mut stream, confirm_block_sequences)| {
            let world = entity.world();
            for sequence in confirm_block_sequences.drain(..) {
                let sequence = VarInt(sequence);
                let ack = play::PlayerActionResponseS2c { sequence };
                compose.unicast(&ack, stream, system_id, &world).unwrap();
            }
        },
    );

    // system!(
    //     "handle_events_player",
    //     world,
    //     &Compose($),
    //     &mut EventQueue,
    //     &NetworkStreamRef,
    //     &InGameName,
    //     &mut Health,
    // )
    // .multi_threaded()
    // .tracing_each_entity(
    //     trace_span!("handle_events_player"),
    //     move |view, (compose, event_queue, stream, ign, health)| {
    //         // todo
    //     },
    // );
}

pub fn reset_event_queue(world: &World) {
    system!("reset_event_queue", world, &mut EventQueue)
        .kind::<pipeline::PostUpdate>()
        .multi_threaded()
        .tracing_each(trace_span!("reset_event_queue"), |event_queue| {
            event_queue.reset();
        });
}

pub fn reset_allocators(world: &World) {
    system!(
        "reset_allocators",
        world,
        &mut ThreadLocalBump($)
    )
    .kind::<pipeline::PostUpdate>()
    .each(|allocator| {
        let span = info_span!("reset_allocators");
        let _enter = span.enter();

        // par iter is 177µs
        allocator.iter_mut().for_each(|allocator| {
            allocator.reset();
        });
    });
}

#[instrument(skip_all, level = "trace")]
fn get_red_hit_packet(id: Entity) -> play::EntityDamageS2c {
    // todo
    play::EntityDamageS2c {
        entity_id: VarInt(id.0 as i32),
        source_type_id: VarInt::default(),
        source_cause_id: VarInt::default(),
        source_direct_id: VarInt::default(),
        source_pos: None,
    }
}
