use std::borrow::Cow;

use azalea_buf::McBufWritable;
use evenio::prelude::*;
use itertools::Itertools;
use tracing::info;
use valence_protocol::{
    math::DVec3,
    nbt::{compound, List},
    packets::{play, play::player_position_look_s2c::PlayerPositionLookFlags},
    BlockPos, ChunkPos,
};

use crate::{chunk::heightmap, KickPlayer, Player, PlayerJoinWorld, GLOBAL};

pub fn player_join_world(
    r: Receiver<PlayerJoinWorld, (EntityId, &mut Player)>,
    mut s: Sender<KickPlayer>,
) {
    let (id, player) = r.query;

    info!("Player {:?} joined the world", id);

    if let Err(e) = inner(player) {
        s.send(KickPlayer {
            target: id,
            reason: format!("Failed to join world: {}", e),
        });
    }
}

fn inner(io: &mut Player) -> anyhow::Result<()> {
    let io = &mut io.packets;

    io.writer.send_game_join_packet()?;

    io.writer.send_packet(&play::PlayerSpawnPositionS2c {
        position: BlockPos::default(),
        angle: 3.0,
    })?;

    io.writer.send_packet(&play::PlayerPositionLookS2c {
        position: DVec3::new(0.0, 3.0, 0.0),
        yaw: 0.0,
        pitch: 0.0,
        flags: PlayerPositionLookFlags::default(),
        teleport_id: 1.into(),
    })?;

    io.writer.send_packet(&play::ChunkRenderDistanceCenterS2c {
        chunk_x: 0.into(),
        chunk_z: 0.into(),
    })?;

    // 27. Chunk Data
    #[allow(clippy::integer_division)]
    let mut chunk = azalea_world::Chunk::default();
    let dimension_height = 384;

    for section in chunk.sections.iter_mut().take(1) {
        // Sections with a block count of 0 are not rendered
        section.block_count = 4096;

        // Set the Palette to be a single value
        let states = &mut section.states;
        states.palette = azalea_world::palette::Palette::SingleValue(2);
    }

    let mut bytes = Vec::new();
    chunk.write_into(&mut bytes)?;

    let map = heightmap(dimension_height, dimension_height - 3);
    let map: Vec<_> = map.into_iter().map(i64::try_from).try_collect()?;

    let mut pkt = play::ChunkDataS2c {
        pos: ChunkPos::new(0, 0),
        heightmaps: Cow::Owned(compound! {
            "MOTION_BLOCKING" => List::Long(map),
        }),
        blocks_and_biomes: &bytes,
        block_entities: Cow::Borrowed(&[]),

        sky_light_mask: Cow::Borrowed(&[]),
        block_light_mask: Cow::Borrowed(&[]),
        empty_sky_light_mask: Cow::Borrowed(&[]),
        empty_block_light_mask: Cow::Borrowed(&[]),
        sky_light_arrays: Cow::Borrowed(&[]),
        block_light_arrays: Cow::Borrowed(&[]),
    };
    for x in -16..=16 {
        for z in -16..=16 {
            pkt.pos = ChunkPos::new(x, z);
            io.writer.send_packet(&pkt)?;
        }
    }

    GLOBAL
        .player_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    Ok(())
}
