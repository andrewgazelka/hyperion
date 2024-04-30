use evenio::prelude::*;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::{instrument, trace};
use valence_protocol::{packets::play, ChunkPos};

use crate::{
    components::{chunks::Chunks, FullEntityPose, LastSentChunk},
    config::CONFIG,
    event::Gametick,
    net::{Compose, Packets},
};

#[instrument(skip_all, level = "trace")]
pub fn send_chunk_updates(
    _: Receiver<Gametick>,
    mut fetcher: Fetcher<(&mut LastSentChunk, &mut FullEntityPose, &Packets)>,
    chunks: Single<&Chunks>,
    compose: Compose,
) {
    let radius = CONFIG.view_distance;
    // chunk updates yay
    fetcher
        .par_iter_mut()
        .for_each(|(last_sent, pose, packets)| {
            let last_sent_chunk = last_sent.chunk;

            let current_chunk = pose.chunk_pos();

            if last_sent_chunk == current_chunk {
                return;
            }

            // center chunk
            let center_chunk = play::ChunkRenderDistanceCenterS2c {
                chunk_x: current_chunk.x.into(),
                chunk_z: current_chunk.z.into(),
            };

            packets.append(&center_chunk, &compose).unwrap();

            last_sent.chunk = current_chunk;

            trace!("sending chunk updates {last_sent:?} -> {current_chunk:?}");

            let last_sent_x_range = last_sent_chunk.x - radius..last_sent_chunk.x + radius;
            let last_sent_z_range = last_sent_chunk.z - radius..last_sent_chunk.z + radius;

            let current_x_range = current_chunk.x - radius..current_chunk.x + radius;
            let current_z_range = current_chunk.z - radius..current_chunk.z + radius;

            // todo: how to do without par bridge
            let added_chunks = current_x_range
                .flat_map(move |x| current_z_range.clone().map(move |z| ChunkPos::new(x, z)))
                .filter(|x| !last_sent_x_range.contains(&x.x) || !last_sent_z_range.contains(&x.z));
            // .par_bridge();

            added_chunks.for_each(|chunk| {
                let Ok(Some(raw)) = chunks.get(chunk, &compose) else {
                    return;
                };

                let mut io_buf = compose.bufs.get_local().borrow_mut();
                let io_buf = &mut *io_buf;
                packets.append_raw(&raw, io_buf);

                trace!("appended chunk {chunk:?}");
            });
        });
}
