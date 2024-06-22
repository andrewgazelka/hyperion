use std::collections::HashSet;

use derive_more::{Deref, DerefMut};
use flecs_ecs::{
    core::{flecs::pipeline::OnUpdate, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World},
    macros::Component,
};
use glam::I16Vec2;
use tracing::{debug, error, instrument};
use valence_protocol::packets::play;

use crate::{
    component::{
        blocks::{Blocks, ChunkData},
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

#[instrument(skip_all, level = "trace")]
pub fn generate_chunk_changes(world: &World) {
    let radius = CONFIG.view_distance as i16;

    world
        .system_named::<(
            &Compose,
            &mut ChunkPosition,
            &Pose,
            &NetworkStreamRef,
            &mut ChunkChanges,
        )>("generate_chunk_changes")
        .kind::<OnUpdate>()
        .term_at(0)
        .multi_threaded()
        .singleton()
        .each(
            move |(compose, last_sent, pose, stream_id, chunk_changes)| {
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

                compose.unicast(&center_chunk, stream_id).unwrap();

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
    world
        .system_named::<(
            &Blocks,
            &AsyncRuntime,
            &Compose,
            &NetworkStreamRef,
            &mut ChunkChanges,
        )>("send_updates")
        .with::<&Play>()
        .kind::<OnUpdate>()
        .multi_threaded()
        .term_at(0)
        .singleton()
        .term_at(1)
        .singleton()
        .term_at(2)
        .singleton()
        .each(|(chunks, tasks, compose, stream_id, chunk_changes)| {
            let mut left_over = Vec::new();

            for &elem in &chunk_changes.changes {
                match chunks.get_cached_or_load(elem, tasks) {
                    Ok(Some(ChunkData::Cached(chunk))) => {
                        compose.io_buf().unicast_raw(chunk, stream_id);
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
        });
}
