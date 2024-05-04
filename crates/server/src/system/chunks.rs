use std::collections::BTreeSet;

use derive_more::{Deref, DerefMut};
use evenio::prelude::*;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::{debug, error, instrument};
use valence_protocol::{packets::play, ChunkPos};

use crate::{
    components::{
        chunks::{Chunks, Tasks},
        FullEntityPose, LastSentChunk,
    },
    config::CONFIG,
    event::Gametick,
    net::{Compose, Packets},
};

#[derive(Component, Deref, DerefMut, Default)]
pub struct ChunkChanges {
    changes: BTreeSet<ChunkPos>,
}

#[instrument(skip_all, level = "trace")]
pub fn generate_changes(
    _: Receiver<Gametick>,
    mut fetcher: Fetcher<(
        &mut LastSentChunk,
        &mut FullEntityPose,
        &mut Packets,
        &mut ChunkChanges,
    )>,
    compose: Compose,
) {
    let radius = CONFIG.view_distance;

    fetcher
        .par_iter_mut()
        .for_each(|(last_sent, pose, packets, chunk_changes)| {
            let last_sent_chunk = last_sent.chunk;

            let current_chunk = pose.chunk_pos();

            if last_sent_chunk == current_chunk {
                return;
            }

            debug!("sending chunk updates {last_sent:?} -> {current_chunk:?}");

            // center chunk
            let center_chunk = play::ChunkRenderDistanceCenterS2c {
                chunk_x: current_chunk.x.into(),
                chunk_z: current_chunk.z.into(),
            };

            packets.append(&center_chunk, &compose).unwrap();

            last_sent.chunk = current_chunk;

            let last_sent_x_range = last_sent_chunk.x - radius..last_sent_chunk.x + radius;
            let last_sent_z_range = last_sent_chunk.z - radius..last_sent_chunk.z + radius;

            let current_x_range = current_chunk.x - radius..current_chunk.x + radius;
            let current_z_range = current_chunk.z - radius..current_chunk.z + radius;

            let added_chunks = current_x_range
                .flat_map(move |x| current_z_range.clone().map(move |z| ChunkPos::new(x, z)))
                .filter(|x| !last_sent_x_range.contains(&x.x) || !last_sent_z_range.contains(&x.z));

            for chunk in added_chunks {
                chunk_changes.insert(chunk);
            }
        });
}

#[instrument(skip_all, level = "trace")]
pub fn send_updates(
    _: Receiver<Gametick>,
    mut fetcher: Fetcher<(&mut Packets, &mut ChunkChanges)>,
    chunks: Single<&Chunks>,
    tasks: Single<&Tasks>,
) {
    fetcher.par_iter_mut().for_each(|(packets, chunk_changes)| {
        let mut left_over = Vec::new();

        for &elem in &chunk_changes.changes {
            match chunks.get_cached_or_load(elem, &tasks) {
                Ok(Some(chunk)) => {
                    packets.append_raw(&chunk);
                    continue;
                }
                Ok(None) => {
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
