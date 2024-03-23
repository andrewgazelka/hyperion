use std::{borrow::Cow, io::Write};

use chunk::{
    bit_width,
    chunk::{BiomeContainer, BlockStateContainer, SECTION_BLOCK_COUNT},
    palette::{BlockGetter, DirectEncoding},
};
use evenio::prelude::*;
use itertools::Itertools;
use tracing::info;
use valence_protocol::{
    math::DVec3,
    nbt::{compound, List},
    packets::{play, play::player_position_look_s2c::PlayerPositionLookFlags},
    BlockPos, BlockState, ChunkPos, Encode,
};
use valence_registry::{biome::BiomeId, RegistryIdx};

use crate::{chunk::heightmap, KickPlayer, Player, PlayerJoinWorld, GLOBAL};

struct AllBlock2;

impl BlockGetter for AllBlock2 {
    fn get_state(&self, _x: usize, _y: usize, _z: usize) -> u64 {
        2
    }
}

pub fn player_join_world(
    r: Receiver<PlayerJoinWorld, (EntityId, &mut Player)>,
    mut s: Sender<KickPlayer>,
) {
    let (id, player) = r.query;

    info!("Player {} joined the world", player.name);

    if let Err(e) = inner(player) {
        s.send(KickPlayer {
            target: id,
            reason: format!("Failed to join world: {e}"),
        });
    }
}

fn write_block_states(states: BlockStateContainer, writer: &mut impl Write) -> anyhow::Result<()> {
    states.encode_mc_format(
        writer,
        |b| b.to_raw().into(),
        4,
        8,
        bit_width(BlockState::max_raw().into()),
    )?;
    Ok(())
}

fn write_biomes(biomes: BiomeContainer, writer: &mut impl Write) -> anyhow::Result<()> {
    biomes.encode_mc_format(
        writer,
        |b| b.to_index() as u64,
        0,
        3,
        6, // bit_width(info.biome_registry_len - 1),
    )?;
    Ok(())
}

fn air_section() -> Vec<u8> {
    let mut section_bytes = Vec::new();
    0_u16.encode(&mut section_bytes).unwrap();

    let block_states = BlockStateContainer::Single(BlockState::AIR);
    write_block_states(block_states, &mut section_bytes).unwrap();

    let biomes = BiomeContainer::Single(BiomeId::DEFAULT);
    write_biomes(biomes, &mut section_bytes).unwrap();

    section_bytes
}

fn ground_section() -> Vec<u8> {
    let mut section_bytes = Vec::new();

    let number_blocks: u16 = 16 * 16;
    number_blocks.encode(&mut section_bytes).unwrap();

    let blocks: [_; SECTION_BLOCK_COUNT] = std::array::from_fn(|i| {
        if i < 16 * 16 {
            BlockState::GRASS_BLOCK
        } else {
            BlockState::AIR
        }
    });

    let block_states = BlockStateContainer::Direct(Box::new(blocks));

    write_block_states(block_states, &mut section_bytes).unwrap();

    let biomes = BiomeContainer::Single(BiomeId::DEFAULT);
    write_biomes(biomes, &mut section_bytes).unwrap();

    section_bytes
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

    let section_count = 384 / 16;
    let air_section = air_section();
    let ground_section = ground_section();

    let mut bytes = Vec::new();

    bytes.extend_from_slice(&ground_section);

    for _ in (0..section_count).skip(1) {
        bytes.extend_from_slice(&air_section);
    }

    let dimension_height = 384;

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
