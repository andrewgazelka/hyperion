use std::collections::HashSet;

use derive_more::{Deref, DerefMut};
use evenio::prelude::*;
use glam::I16Vec2;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::{debug, error, instrument};
use valence_protocol::packets::play;

use crate::{
    components::{
        chunks::{ChunkData, Chunks, Tasks},
        ChunkLocation, FullEntityPose,
    },
    config::CONFIG,
    event::Gametick,
    net::{Compose, StreamId},
};

#[derive(Component, Deref, DerefMut, Default)]
pub struct ChunkChanges {
    changes: HashSet<I16Vec2>,
}

#[instrument(skip_all, level = "trace")]
pub fn generate_chunk_changes(
    _: Receiver<Gametick>,
    mut fetcher: Fetcher<(
        &mut ChunkLocation,
        &mut FullEntityPose,
        &mut StreamId,
        &mut ChunkChanges,
    )>,
    compose: Compose,
) {
    let radius = CONFIG.view_distance as i16;

    fetcher
        .par_iter_mut()
        .for_each(|(last_sent, pose, packets, chunk_changes)| {
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

            compose.unicast(&center_chunk, packets).unwrap();

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
        });
}

#[instrument(skip_all, level = "trace")]
pub fn send_updates(
    _: Receiver<Gametick>,
    mut fetcher: Fetcher<(&mut StreamId, &mut ChunkChanges)>,
    chunks: Single<&Chunks>,
    tasks: Single<&Tasks>,
    compose: Compose,
) {
    fetcher
        .par_iter_mut()
        .for_each(|(stream_id, chunk_changes)| {
            let mut left_over = Vec::new();

            for &elem in &chunk_changes.changes {
                match chunks.get_cached_or_load(elem, &tasks) {
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
