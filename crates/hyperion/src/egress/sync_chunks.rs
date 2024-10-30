use std::cmp::Ordering;

use derive_more::derive::{Deref, DerefMut};
use flecs_ecs::prelude::*;
use glam::IVec2;
use tracing::{error, trace_span};
use valence_protocol::{
    packets::play::{self},
    ChunkPos,
};

use crate::{
    config::Config,
    net::{Compose, DataBundle, NetworkStreamRef},
    simulation::{
        blocks::{Blocks, GetChunk},
        ChunkPosition, Play, Position,
    },
    system_registry::{GENERATE_CHUNK_CHANGES, SEND_FULL_LOADED_CHUNKS},
    util::TracingExt,
};

#[derive(Component, Deref, DerefMut, Default)]
pub struct ChunkSendQueue {
    changes: Vec<IVec2>,
}

#[derive(Component)]
pub struct SyncChunksModule;

impl Module for SyncChunksModule {
    fn module(world: &World) {
        world.component::<ChunkSendQueue>();

        let radius = world.get::<&Config>(|config| config.view_distance);
        let liberal_radius = radius + 2;

        let system_id = GENERATE_CHUNK_CHANGES;

        system!(
            "generate_chunk_changes",
            world,
            &Compose($),
            &mut ChunkPosition,
            &Position,
            &NetworkStreamRef,
            &mut ChunkSendQueue,
        )
        .with::<Play>()
        .kind::<flecs::pipeline::OnUpdate>()
        .multi_threaded()
        .tracing_each_entity(
            trace_span!("generate_chunk_changes"),
            move |entity, (compose, last_sent, pose, &stream_id, chunk_changes)| {
                let world = entity.world();

                let last_sent_chunk = last_sent.0;

                let current_chunk = pose.to_chunk();

                if last_sent_chunk == current_chunk {
                    return;
                }

                // center chunk
                let center_chunk = play::ChunkRenderDistanceCenterS2c {
                    chunk_x: current_chunk.x.into(),
                    chunk_z: current_chunk.y.into(),
                };

                if let Err(e) = compose.unicast(&center_chunk, stream_id, system_id, &world) {
                    error!(
                        "failed to send chunk render distance center packet: {e}. Chunk location: \
                         {current_chunk:?}"
                    );
                    return;
                }

                last_sent.0 = current_chunk;

                let last_sent_range_x = (last_sent_chunk.x - radius)..(last_sent_chunk.x + radius);
                let last_sent_range_z = (last_sent_chunk.y - radius)..(last_sent_chunk.y + radius);

                let current_range_x = (current_chunk.x - radius)..(current_chunk.x + radius);
                let current_range_z = (current_chunk.y - radius)..(current_chunk.y + radius);

                let current_range_liberal_x =
                    (current_chunk.x - liberal_radius)..(current_chunk.x + liberal_radius);
                let current_range_liberal_z =
                    (current_chunk.y - liberal_radius)..(current_chunk.y + liberal_radius);

                chunk_changes.retain(|elem| {
                    current_range_liberal_x.contains(&elem.x)
                        && current_range_liberal_z.contains(&elem.y)
                });

                let removed_chunks = last_sent_range_x
                    .clone()
                    .flat_map(|x| last_sent_range_z.clone().map(move |z| IVec2::new(x, z)))
                    .filter(|pos| {
                        !current_range_x.contains(&pos.x) || !current_range_z.contains(&pos.y)
                    });

                let mut bundle = DataBundle::new(compose);

                for chunk in removed_chunks {
                    let pos = ChunkPos::new(chunk.x, chunk.y);
                    let unload_chunk = play::UnloadChunkS2c { pos };

                    bundle.add_packet(&unload_chunk, &world).unwrap();

                    // if let Err(e) = compose.unicast(&unload_chunk, stream_id, system_id, &world) {
                    //     error!(
                    //         "Failed to send unload chunk packet: {e}. Chunk location: {chunk:?}"
                    //     );
                    // }
                }

                bundle.send(&world, stream_id, system_id).unwrap();

                let added_chunks = current_range_x
                    .flat_map(move |x| current_range_z.clone().map(move |z| IVec2::new(x, z)))
                    .filter(|pos| {
                        !last_sent_range_x.contains(&pos.x) || !last_sent_range_z.contains(&pos.y)
                    });

                let mut num_chunks_added = 0;

                // drain all chunks not in current_{x,z} range

                for chunk in added_chunks {
                    chunk_changes.push(chunk);
                    num_chunks_added += 1;
                }

                if num_chunks_added > 0 {
                    // remove further than radius

                    // commented out because it can break things
                    // todo: re-add but have better check so we don't prune things and then never
                    // send them
                    // chunk_changes.retain(|elem| {
                    //     let elem = elem.distance_squared(current_chunk);
                    //     elem <= r2_very_liberal
                    // });

                    chunk_changes.sort_unstable_by(|a, b| {
                        let r1 = a.distance_squared(current_chunk);
                        let r2 = b.distance_squared(current_chunk);

                        // reverse because we want to get the closest chunks first and we are poping from the end
                        match r1.cmp(&r2).reverse() {
                            Ordering::Less => Ordering::Less,
                            Ordering::Greater => Ordering::Greater,

                            // so we can dedup properly (without same element could be scattered around)
                            Ordering::Equal => a.to_array().cmp(&b.to_array()),
                        }
                    });
                    chunk_changes.dedup();
                }
            },
        );

        let system_id = SEND_FULL_LOADED_CHUNKS;

        system!("send_full_loaded_chunks", world, &Blocks($), &Compose($), &NetworkStreamRef, &mut ChunkSendQueue)
            .with::<Play>()
            .kind::<flecs::pipeline::OnUpdate>()
            .multi_threaded()
            .each_entity(
                move |entity, (chunks, compose, &stream_id, queue)| {
                    const MAX_CHUNKS_PER_TICK: usize = 16;

                    let world = entity.world();

                    let last = None;

                    let mut iter_count = 0;

                    let mut bundle = DataBundle::new(compose);

                    #[expect(
                        clippy::cast_possible_wrap,
                        reason = "realistically queue.changes.len() will never be large enough to wrap"
                    )]
                    let mut idx = (queue.changes.len() as isize) - 1;

                    while idx >= 0 {
                        #[expect(clippy::cast_sign_loss, reason = "we are checking if < 0")]
                        let Some(elem) = queue.changes.get(idx as usize).copied() else {
                            // should never happen but we do not want to panic if wrong
                            // logic/assumptions are made
                            error!("failed to get element from queue.changes");
                            continue;
                        };

                        // de-duplicate. todo: there are cases where duplicate will not be removed properly
                        // since sort is unstable
                        if last == Some(elem) {
                            #[expect(clippy::cast_sign_loss, reason = "we are checking if < 0")]
                            queue.changes.swap_remove(idx as usize);
                            idx -= 1;
                            continue;
                        }

                        if iter_count >= MAX_CHUNKS_PER_TICK {
                            break;
                        }

                        match chunks.get_cached_or_load(elem) {
                            GetChunk::Loaded(chunk) => {
                                bundle.add_raw(&chunk.base_packet_bytes);

                                for packet in chunk.original_delta_packets() {
                                    if let Err(e) = bundle.add_packet(packet, &world) {
                                        error!("failed to send chunk delta packet: {e}");
                                        return;
                                    }
                                }

                                iter_count += 1;
                                #[expect(clippy::cast_sign_loss, reason = "we are checking if < 0")]
                                queue.changes.swap_remove(idx as usize);
                            }
                            GetChunk::Loading => {}
                        }

                        idx -= 1;
                    }

                    bundle.send(&world, stream_id, system_id).unwrap();
                },
            );
    }
}
