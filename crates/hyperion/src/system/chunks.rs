use std::collections::HashSet;

use derive_more::{Deref, DerefMut};
use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TermBuilderImpl, World},
    macros::{system, Component},
};
use glam::I16Vec2;
use tracing::{debug, error, info_span, instrument};
use valence_protocol::packets::play;

use crate::{
    component::{
        blocks::{ChunkData, MinecraftWorld},
        ChunkPosition, Play, Pose,
    },
    config::CONFIG,
    net::{Compose, NetworkStreamRef},
    runtime::AsyncRuntime,
};

#[derive(Component, Deref, DerefMut, Default)]
pub struct ChunkChanges {
    changes: HashSet<I16Vec2>,
}

pub fn load_pending(world: &World) {
    system!(
        "load_pending",
        world,
        &mut MinecraftWorld($),
    )
    .each_iter(|iter, _, blocks| {
        let span = tracing::trace_span!("load_pending");
        let _enter = span.enter();
        blocks.load_pending(&iter.world());
    });
}

#[instrument(skip_all, level = "trace")]
pub fn generate_chunk_changes(world: &World) {
    let radius = CONFIG.view_distance as i16;

    system!(
        "generate_chunk_changes",
        world,
        &Compose($),
        &mut ChunkPosition,
        &Pose,
        &NetworkStreamRef,
        &mut ChunkChanges,
    )
    .multi_threaded()
    .each_iter(
        move |it, _, (compose, last_sent, pose, stream_id, chunk_changes)| {
            let world = it.world();

            let last_sent_chunk = last_sent.0;

            let current_chunk = pose.chunk_pos();

            if last_sent_chunk == current_chunk {
                return;
            }

            debug!("sending chunk updates {last_sent:?} -> {current_chunk:?}");

            // center chunk
            let center_chunk = play::ChunkRenderDistanceCenterS2c {
                chunk_x: i32::from(current_chunk.x).into(),
                chunk_z: i32::from(current_chunk.y).into(),
            };

            compose.unicast(&center_chunk, stream_id, &world).unwrap();

            last_sent.0 = current_chunk;

            let last_sent_x_range = last_sent_chunk.x - radius..last_sent_chunk.x + radius;
            let last_sent_z_range = last_sent_chunk.y - radius..last_sent_chunk.y + radius;

            let current_x_range = current_chunk.x - radius..current_chunk.x + radius;
            let current_z_range = current_chunk.y - radius..current_chunk.y + radius;

            let added_chunks = current_x_range
                .flat_map(move |x| current_z_range.clone().map(move |z| I16Vec2::new(x, z)))
                .filter(|pos| {
                    !last_sent_x_range.contains(&pos.x) || !last_sent_z_range.contains(&pos.y)
                });

            for chunk in added_chunks {
                chunk_changes.insert(chunk);
            }
        },
    );
}

pub fn send_updates(world: &World) {
    system!(
        "send_updates",
        world,
        &MinecraftWorld($),
        &AsyncRuntime($),
        &Compose($),
        &NetworkStreamRef,
        &mut ChunkChanges,
    )
    .with::<&Play>()
    .each_iter(
        |iter, _, (chunks, tasks, compose, stream_id, chunk_changes)| {
            let span = info_span!("send_updates");
            let _enter = span.enter();

            let mut left_over = Vec::new();

            let world = iter.world();

            for &elem in &chunk_changes.changes {
                match chunks.get_cached_or_load(elem, tasks, &world) {
                    Ok(Some(ChunkData::Cached(chunk))) => {
                        compose.io_buf().unicast_raw(chunk, stream_id, &world);
                        continue;
                    }
                    Ok(Some(ChunkData::Task(..)) | None) => {
                        left_over.push(elem);
                        continue;
                    }
                    Err(err) => {
                        error!("failed to get chunk {elem:?}: {err}");
                        continue;
                    }
                }
            }

            chunk_changes.changes = left_over.into_iter().collect();
        },
    );
}
