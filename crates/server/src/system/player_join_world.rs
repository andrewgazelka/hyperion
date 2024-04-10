use std::{borrow::Cow, collections::BTreeSet, io::Write};

use chunk::{
    bit_width,
    chunk::{BiomeContainer, BlockStateContainer, SECTION_BLOCK_COUNT},
};
use evenio::prelude::*;
use itertools::Itertools;
use tracing::{debug, info, instrument};
use valence_protocol::{
    game_mode::OptGameMode,
    ident,
    math::DVec3,
    nbt::{compound, List},
    packets::{
        play,
        play::{
            player_list_s2c::PlayerListActions,
            player_position_look_s2c::PlayerPositionLookFlags,
            team_s2c::{CollisionRule, Mode, NameTagVisibility, TeamColor, TeamFlags},
            GameJoinS2c,
        },
    },
    text::IntoText,
    BlockPos, BlockState, ByteAngle, ChunkPos, Encode, FixedArray, GameMode, Ident, PacketEncoder,
    VarInt,
};
use valence_registry::{biome::BiomeId, BiomeRegistry, RegistryCodec, RegistryIdx};

use crate::{
    bits::BitStorage,
    chunk::heightmap,
    config,
    global::Global,
    net::Encoder,
    singleton::{encoder::Broadcast, player_lookup::PlayerUuidLookup},
    system::init_entity::spawn_packet,
    FullEntityPose, MinecraftEntity, Player, PlayerJoinWorld, Uuid,
};

#[derive(Query, Debug)]
pub(crate) struct EntityQuery<'a> {
    id: EntityId,
    uuid: &'a Uuid,
    pose: &'a FullEntityPose,
    _player: With<&'static MinecraftEntity>,
}

// todo: clean up player_join_world; the file is super super super long and hard to understand
#[instrument(skip_all)]
pub fn player_join_world(
    // todo: I doubt &mut Player will work here due to aliasing
    r: Receiver<PlayerJoinWorld, (EntityId, &Player, &Uuid, &mut Encoder)>,
    entities: Fetcher<EntityQuery>,
    global: Single<&Global>,
    players: Fetcher<(EntityId, &Player, &Uuid, &FullEntityPose)>,
    lookup: Single<&mut PlayerUuidLookup>,
    mut broadcast: Single<&mut Broadcast>,
) {
    static CACHED_DATA: once_cell::sync::OnceCell<bytes::Bytes> = once_cell::sync::OnceCell::new();

    let compression_level = global.0.shared.compression_level;

    let cached_data = CACHED_DATA.get_or_init(|| {
        let mut encoder = PacketEncoder::new();
        encoder.set_compression(compression_level);

        info!("Caching world data for new players");
        inner(&mut encoder).unwrap();

        let bytes = encoder.take();
        bytes.freeze()
    });

    let buf = broadcast.get_round_robin();

    let (id, current_player, uuid, encoder) = r.query;

    lookup.0.insert(uuid.0, id);

    let entries = &[play::player_list_s2c::PlayerListEntry {
        player_uuid: uuid.0,
        username: &current_player.name,
        properties: Cow::Borrowed(&[]),
        chat_data: None,
        listed: true,
        ping: 0,
        game_mode: GameMode::Creative,
        display_name: Some("SomeBot".into_cow_text()),
    }];

    let info = play::PlayerListS2c {
        actions: PlayerListActions::default().with_add_player(true),
        entries: Cow::Borrowed(entries),
    };

    buf.append_packet(&info).unwrap();

    let text = valence_protocol::packets::play::GameMessageS2c {
        chat: format!("{} joined the world", current_player.name).into_cow_text(),
        overlay: false,
    };

    buf.append_packet(&text).unwrap();

    encoder.append(cached_data);

    for entity in entities {
        let pkt = spawn_packet(entity.id, *entity.uuid, entity.pose);
        encoder.encode(&pkt).unwrap();
    }

    // todo: cache
    let entries = players
        .iter()
        .map(
            |(_, player, uuid, _)| play::player_list_s2c::PlayerListEntry {
                player_uuid: uuid.0,
                username: &player.name,
                properties: Cow::Borrowed(&[]),
                chat_data: None,
                listed: true,
                ping: 0,
                game_mode: GameMode::Creative,
                display_name: Some("SomeBot".into_cow_text()),
            },
        )
        .collect::<Vec<_>>();

    let player_names = players
        .iter()
        .map(|(_, player, ..)| &*player.name)
        .collect::<Vec<_>>();

    encoder
        .encode(&play::TeamS2c {
            team_name: "no_tag",
            mode: Mode::AddEntities {
                entities: player_names,
            },
        })
        .unwrap();

    let current_name = &*current_player.name;

    buf.append_packet(&play::TeamS2c {
        team_name: "no_tag",
        mode: Mode::AddEntities {
            entities: vec![current_name],
        },
    })
    .unwrap();

    encoder
        .encode(&play::PlayerListS2c {
            actions: PlayerListActions::default().with_add_player(true),
            entries: Cow::Owned(entries),
        })
        .unwrap();

    let tick = global.tick;
    let time_of_day = tick % 24000;

    encoder
        .encode(&play::WorldTimeUpdateS2c {
            world_age: tick,
            time_of_day,
        })
        .unwrap();

    // todo: cache
    for (id, _, uuid, pose) in &players {
        let entity_id = VarInt(id.index().0 as i32);

        let pkt = play::PlayerSpawnS2c {
            entity_id,
            player_uuid: uuid.0,
            position: pose.position.as_dvec3(),
            yaw: ByteAngle::from_degrees(pose.yaw),
            pitch: ByteAngle::from_degrees(pose.pitch),
        };
        encoder.encode(&pkt).unwrap();
    }

    global
        .0
        .shared
        .player_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let entity_id = VarInt(id.index().0 as i32);

    let dx = fastrand::f64().mul_add(10.0, -5.0);
    let dz = fastrand::f64().mul_add(10.0, -5.0);

    let spawn_player = play::PlayerSpawnS2c {
        entity_id,
        player_uuid: uuid.0,
        position: DVec3::new(dx, 30.0, dz),
        yaw: ByteAngle(0),
        pitch: ByteAngle(0),
    };

    buf.append_packet(&spawn_player).unwrap();

    encoder.deallocate_on_process();

    info!("Player {} joined the world", current_player.name);
}

fn write_block_states(states: &BlockStateContainer, writer: &mut impl Write) -> anyhow::Result<()> {
    states.encode_mc_format(
        writer,
        |b| b.to_raw().into(),
        4,
        8,
        bit_width(BlockState::max_raw().into()),
    )?;
    Ok(())
}

fn write_biomes(biomes: &BiomeContainer, writer: &mut impl Write) -> anyhow::Result<()> {
    biomes.encode_mc_format(
        writer,
        |b| b.to_index() as u64,
        0,
        3,
        6, // bit_width(info.biome_registry_len - 1),
    )?;
    Ok(())
}

trait Array3d {
    type Item;
    #[expect(dead_code, reason = "unused")]
    fn get3(&self, x: usize, y: usize, z: usize) -> &Self::Item;
    fn get3_mut(&mut self, x: usize, y: usize, z: usize) -> &mut Self::Item;
}

#[expect(
    clippy::indexing_slicing,
    reason = "the signature of the trait allows for panics"
)]
impl<T, const N: usize> Array3d for [T; N] {
    type Item = T;

    fn get3(&self, x: usize, y: usize, z: usize) -> &Self::Item {
        &self[x + z * 16 + y * 16 * 16]
    }

    fn get3_mut(&mut self, x: usize, y: usize, z: usize) -> &mut Self::Item {
        &mut self[x + z * 16 + y * 16 * 16]
    }
}

pub fn send_keep_alive(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    let pkt = valence_protocol::packets::play::KeepAliveS2c {
        // The ID can be set to zero because it doesn't matter
        id: 0,
    };

    encoder.append_packet(&pkt)?;

    Ok(())
}

pub fn send_game_join_packet(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    // recv ack

    let codec = RegistryCodec::default();

    let registry_codec = crate::net::registry_codec_raw(&codec)?;

    let dimension_names: BTreeSet<Ident<Cow<str>>> = codec
        .registry(BiomeRegistry::KEY)
        .iter()
        .map(|value| value.name.as_str_ident().into())
        .collect();

    let dimension_name = ident!("overworld");
    // let dimension_name: Ident<Cow<str>> = chunk_layer.dimension_type_name().into();

    let pkt = GameJoinS2c {
        entity_id: 0,
        is_hardcore: false,
        dimension_names: Cow::Owned(dimension_names),
        registry_codec: Cow::Borrowed(&registry_codec),
        max_players: config::CONFIG.max_players.into(),
        view_distance: config::CONFIG.view_distance.into(), // max view distance
        simulation_distance: config::CONFIG.simulation_distance.into(),
        reduced_debug_info: false,
        enable_respawn_screen: false,
        dimension_name: dimension_name.into(),
        hashed_seed: 0,
        game_mode: GameMode::Creative,
        is_flat: false,
        last_death_location: None,
        portal_cooldown: 60.into(),
        previous_game_mode: OptGameMode(Some(GameMode::Creative)),
        dimension_type_name: "minecraft:overworld".try_into()?,
        is_debug: false,
    };

    encoder.append_packet(&pkt)?;

    Ok(())
}

fn send_commands(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    // https://wiki.vg/Command_Data
    use valence_protocol::packets::play::command_tree_s2c::{
        CommandTreeS2c, Node, NodeData, Parser,
    };

    // id 0
    let root = Node {
        data: NodeData::Root,
        executable: false,
        children: vec![VarInt(1), VarInt(3)],
        redirect_node: None,
    };

    // id 1
    let spawn = Node {
        data: NodeData::Literal {
            name: "spawn".to_owned(),
        },
        executable: true,
        children: vec![VarInt(2)],
        redirect_node: None,
    };

    // id 2
    let spawn_arg = Node {
        data: NodeData::Argument {
            name: "position".to_owned(),
            parser: Parser::BlockPos,
            suggestion: None,
        },
        executable: true,
        children: vec![],
        redirect_node: None,
    };

    // id 3 = "killall"
    let clear = Node {
        data: NodeData::Literal {
            name: "ka".to_owned(),
        },
        executable: true,
        children: vec![],
        redirect_node: None,
    };

    // id 4 = "ka" replace with "killall"
    // let ka = Node {
    //     data: NodeData::Literal {
    //         name: "ka".to_owned(),
    //     },
    //     executable: false,
    //     children: vec![],
    //     redirect_node: Some(VarInt(3)),
    // };

    encoder.append_packet(&CommandTreeS2c {
        commands: vec![root, spawn, spawn_arg, clear],
        root_index: VarInt(0),
    })?;

    Ok(())
}

fn air_section() -> Vec<u8> {
    let mut section_bytes = Vec::new();
    0_u16.encode(&mut section_bytes).unwrap();

    let block_states = BlockStateContainer::Single(BlockState::AIR);
    write_block_states(&block_states, &mut section_bytes).unwrap();

    let biomes = BiomeContainer::Single(BiomeId::DEFAULT);
    write_biomes(&biomes, &mut section_bytes).unwrap();

    section_bytes
}

fn stone_section() -> Vec<u8> {
    let mut section_bytes = Vec::new();
    SECTION_BLOCK_COUNT.encode(&mut section_bytes).unwrap();

    let blocks = [BlockState::STONE; { SECTION_BLOCK_COUNT as usize }];
    let block_states = BlockStateContainer::Direct(Box::new(blocks));
    write_block_states(&block_states, &mut section_bytes).unwrap();

    let biomes = BiomeContainer::Single(BiomeId::DEFAULT);
    write_biomes(&biomes, &mut section_bytes).unwrap();

    section_bytes
}

fn ground_section() -> Vec<u8> {
    let mut section_bytes = Vec::new();

    let number_blocks: u16 = 16 * 16;
    number_blocks.encode(&mut section_bytes).unwrap();

    let mut blocks = [BlockState::AIR; { SECTION_BLOCK_COUNT as usize }];

    let surface_blocks = [
        BlockState::END_STONE,
        BlockState::SAND,
        BlockState::GRAVEL,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
        BlockState::END_STONE,
    ];

    let mut rnd = rand::thread_rng();

    for x in 0..16 {
        for z in 0..16 {
            // let dist_from_center = (x as f64 - 8.0).hypot(z as f64 - 8.0);

            // based on x and z
            // should be highest at center of chunk
            // let height = (16.0 - dist_from_center) * 0.5 + 3.0;
            let height = 5;
            let height = height.min(16);
            for y in 0..height {
                use rand::seq::SliceRandom;
                let block = surface_blocks.choose(&mut rnd).unwrap();
                *blocks.get3_mut(x, y, z) = *block;
            }
        }
    }

    let block_states = BlockStateContainer::Direct(Box::new(blocks));

    write_block_states(&block_states, &mut section_bytes).unwrap();

    let biomes = BiomeContainer::Single(BiomeId::DEFAULT);
    write_biomes(&biomes, &mut section_bytes).unwrap();

    section_bytes
}

fn inner(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    send_game_join_packet(encoder)?;

    encoder.append_packet(&play::ChunkRenderDistanceCenterS2c {
        chunk_x: 0.into(),
        chunk_z: 0.into(),
    })?;

    let section_count = 384 / 16_usize;
    let air_section = air_section();
    let ground_section = ground_section();
    let stone_section = stone_section();

    let mut bytes = Vec::new();

    bytes.extend_from_slice(&stone_section);
    bytes.extend_from_slice(&stone_section);
    bytes.extend_from_slice(&stone_section);
    bytes.extend_from_slice(&stone_section);
    bytes.extend_from_slice(&ground_section);

    // 2048 bytes per section -> long count = 2048 / 8 = 256
    let sky_light_array = FixedArray([0xFF_u8; 2048]);
    let sky_light_arrays = vec![sky_light_array; section_count + 2];

    for _ in (0..section_count).skip(5) {
        bytes.extend_from_slice(&air_section);
    }

    let dimension_height = 384;

    let map = heightmap(dimension_height, dimension_height - 3);
    let map: Vec<_> = map.into_iter().map(i64::try_from).try_collect()?;

    // convert section_count + 2 0b1s into u64 array
    let mut bits = BitStorage::new(1, section_count + 2, None).unwrap();

    for i in 0..section_count + 2 {
        bits.set(i, 1);
    }

    let mut pkt = play::ChunkDataS2c {
        pos: ChunkPos::new(0, 0),
        heightmaps: Cow::Owned(compound! {
            "MOTION_BLOCKING" => List::Long(map),
        }),
        blocks_and_biomes: &bytes,
        block_entities: Cow::Borrowed(&[]),

        sky_light_mask: Cow::Owned(bits.into_data()),
        block_light_mask: Cow::Borrowed(&[]),
        empty_sky_light_mask: Cow::Borrowed(&[]),
        empty_block_light_mask: Cow::Borrowed(&[]),
        sky_light_arrays: Cow::Owned(sky_light_arrays),
        block_light_arrays: Cow::Borrowed(&[]),
    };
    for x in -16..=16 {
        for z in -16..=16 {
            pkt.pos = ChunkPos::new(x, z);
            encoder.append_packet(&pkt)?;
        }
    }

    send_commands(encoder)?;

    encoder.append_packet(&play::PlayerSpawnPositionS2c {
        position: BlockPos::default(),
        angle: 3.0,
    })?;

    let dx = fastrand::f64().mul_add(10.0, -5.0);
    let dz = fastrand::f64().mul_add(10.0, -5.0);

    encoder.append_packet(&play::PlayerPositionLookS2c {
        position: DVec3::new(dx, 30.0, dz),
        yaw: 0.0,
        pitch: 0.0,
        flags: PlayerPositionLookFlags::default(),
        teleport_id: 1.into(),
    })?;

    encoder.append_packet(&play::TeamS2c {
        team_name: "no_tag",
        mode: Mode::CreateTeam {
            team_display_name: Cow::default(),
            friendly_flags: TeamFlags::default(),
            name_tag_visibility: NameTagVisibility::Never,
            collision_rule: CollisionRule::Always,
            team_color: TeamColor::Black,
            team_prefix: Cow::default(),
            team_suffix: Cow::default(),
            entities: vec![],
        },
    })?;

    if let Some(diameter) = config::CONFIG.border_diameter {
        debug!("Setting world border to diameter {}", diameter);

        encoder.append_packet(&play::WorldBorderInitializeS2c {
            x: 0.0,
            z: 0.0,
            old_diameter: diameter,
            new_diameter: diameter,
            duration_millis: 1.into(),
            portal_teleport_boundary: 29_999_984.into(),
            warning_blocks: 50.into(),
            warning_time: 200.into(),
        })?;

        encoder.append_packet(&play::WorldBorderSizeChangedS2c { diameter })?;

        encoder.append_packet(&play::WorldBorderCenterChangedS2c {
            x_pos: 0.0,
            z_pos: 0.0,
        })?;
    }

    Ok(())
}
