use std::{borrow::Cow, collections::BTreeSet, io::Write};

use anyhow::Context;
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
    nbt::{compound, Compound, List},
    packets::{
        play,
        play::{
            entity_equipment_update_s2c::EquipmentEntry,
            player_list_s2c::PlayerListActions,
            player_position_look_s2c::PlayerPositionLookFlags,
            team_s2c::{CollisionRule, Mode, NameTagVisibility, TeamColor, TeamFlags},
            GameJoinS2c,
        },
    },
    text::IntoText,
    BlockPos, BlockState, ByteAngle, ChunkPos, Encode, FixedArray, GameMode, Ident, ItemKind,
    ItemStack, PacketEncoder, VarInt,
};
use valence_registry::{biome::BiomeId, BiomeRegistry, RegistryCodec, RegistryIdx};

use crate::{
    bits::BitStorage,
    chunk::heightmap,
    components::{FullEntityPose, InGameName, MinecraftEntity, Player, Uuid},
    config,
    events::PlayerJoinWorld,
    global::Global,
    net::{Broadcast, IoBuf, Packets},
    singleton::{player_id_lookup::PlayerIdLookup, player_uuid_lookup::PlayerUuidLookup},
    system::init_entity::spawn_packet,
};

#[derive(Query, Debug)]
pub(crate) struct EntityQuery<'a> {
    id: EntityId,
    uuid: &'a Uuid,
    pose: &'a FullEntityPose,
    _player: With<&'static MinecraftEntity>,
}

#[derive(Query)]
pub(crate) struct PlayerJoinWorldQuery<'a> {
    id: EntityId,
    uuid: &'a Uuid,
    pose: &'a FullEntityPose,
    packets: &'a mut Packets,
    name: &'a InGameName,
    _player: With<&'static Player>,
}

#[derive(Query)]
pub(crate) struct PlayerQuery<'a> {
    id: EntityId,
    uuid: &'a Uuid,
    pose: &'a FullEntityPose,
    name: &'a InGameName,
    _player: With<&'static Player>,
}

// todo: clean up player_join_world; the file is super super super long and hard to understand
#[instrument(skip_all)]
pub fn player_join_world(
    r: Receiver<PlayerJoinWorld, PlayerJoinWorldQuery>,
    entities: Fetcher<EntityQuery>,
    global: Single<&Global>,
    players: Fetcher<PlayerQuery>,
    mut uuid_lookup: Single<&mut PlayerUuidLookup>,
    mut id_lookup: Single<&mut PlayerIdLookup>,
    // mut broadcast: Single<&mut Broadcast>,
    mut io: Single<&mut IoBuf>,
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

    info!("got cached data");

    let query = r.query;

    uuid_lookup.insert(query.uuid.0, query.id);
    id_lookup.inner.insert(query.id.index().0 as i32, query.id);

    let boots = ItemStack::new(ItemKind::NetheriteBoots, 1, None);
    let leggings = ItemStack::new(ItemKind::NetheriteLeggings, 1, None);
    let chestplate = ItemStack::new(ItemKind::NetheriteChestplate, 1, None);
    let helmet = ItemStack::new(ItemKind::NetheriteHelmet, 1, None);
    let sword = ItemStack::new(ItemKind::NetheriteSword, 1, None);

    // 0: Mainhand
    // 2: Boots
    // 3: Leggings
    // 4: Chestplate
    // 5: Helmet
    let mainhand = EquipmentEntry {
        slot: 0,
        item: sword,
    };
    let boots = EquipmentEntry {
        slot: 2,
        item: boots,
    };
    let leggings = EquipmentEntry {
        slot: 3,
        item: leggings,
    };
    let chestplate = EquipmentEntry {
        slot: 4,
        item: chestplate,
    };
    let helmet = EquipmentEntry {
        slot: 5,
        item: helmet,
    };

    let equipment = vec![mainhand, boots, leggings, chestplate, helmet];

    let entries = &[play::player_list_s2c::PlayerListEntry {
        player_uuid: query.uuid.0,
        username: query.name,
        properties: Cow::Borrowed(&[]),
        chat_data: None,
        listed: true,
        ping: 0,
        game_mode: GameMode::Survival,
        display_name: Some(query.name.to_string().into_cow_text()),
    }];

    let current_entity_id = VarInt(query.id.index().0 as i32);

    let text = play::GameMessageS2c {
        chat: format!("{} joined the world", query.name).into_cow_text(),
        overlay: false,
    };

    // broadcast.append(&text, &mut io).unwrap();

    let local = query.packets;

    local.append_raw(cached_data, &mut io);

    info!("appending cached data");

    local
        .append(
            &crate::packets::def::EntityEquipmentUpdateS2c {
                entity_id: VarInt(0),
                equipment: Cow::Borrowed(&equipment),
            },
            &mut io,
        )
        .unwrap();

    let actions = PlayerListActions::default()
        .with_add_player(true)
        .with_update_listed(true)
        .with_update_display_name(true);

    let info = play::PlayerListS2c {
        actions,
        entries: Cow::Borrowed(entries),
    };

    // broadcast.append(&info, &mut io).unwrap();

    for entity in entities {
        let pkt = spawn_packet(entity.id, *entity.uuid, entity.pose);
        local.append(&pkt, &mut io).unwrap();
    }

    // todo: cache
    let entries = players
        .iter()
        .map(|query| play::player_list_s2c::PlayerListEntry {
            player_uuid: query.uuid.0,
            username: query.name,
            properties: Cow::Borrowed(&[]),
            chat_data: None,
            listed: true,
            ping: 20,
            game_mode: GameMode::Survival,
            display_name: Some(query.name.to_string().into_cow_text()),
        })
        .collect::<Vec<_>>();

    let player_names: Vec<_> = players
        .iter()
        .map(|query| &***query.name) // todo: lol
        .collect();

    local
        .append(
            &play::TeamS2c {
                team_name: "no_tag",
                mode: Mode::AddEntities {
                    entities: player_names,
                },
            },
            &mut io,
        )
        .unwrap();

    let current_name = query.name;

    // broadcast
    //     .append(
    //         &play::TeamS2c {
    //             team_name: "no_tag",
    //             mode: Mode::AddEntities {
    //                 entities: vec![current_name],
    //             },
    //         },
    //         &mut io,
    //     )
    //     .unwrap();

    local
        .append(
            &play::PlayerListS2c {
                actions,
                entries: Cow::Owned(entries),
            },
            &mut io,
        )
        .unwrap();

    let tick = global.tick;
    let time_of_day = tick % 24000;

    local
        .append(
            &play::WorldTimeUpdateS2c {
                world_age: tick,
                time_of_day,
            },
            &mut io,
        )
        .unwrap();

    // todo: cache
    for current_query in &players {
        let id = current_query.id;
        let pose = current_query.pose;
        let uuid = current_query.uuid;

        let entity_id = VarInt(id.index().0 as i32);

        let pkt = play::PlayerSpawnS2c {
            entity_id,
            player_uuid: uuid.0,
            position: pose.position.as_dvec3(),
            yaw: ByteAngle::from_degrees(pose.yaw),
            pitch: ByteAngle::from_degrees(pose.pitch),
        };

        local.append(&pkt, &mut io).unwrap();

        let pkt = crate::packets::def::EntityEquipmentUpdateS2c {
            entity_id,
            equipment: Cow::Borrowed(&equipment),
        };
        local.append(&pkt, &mut io).unwrap();
    }

    global
        .0
        .shared
        .player_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let spawn_player = play::PlayerSpawnS2c {
        entity_id: current_entity_id,
        player_uuid: query.uuid.0,
        position: query.pose.position.as_dvec3(),
        yaw: ByteAngle::from_degrees(query.pose.yaw),
        pitch: ByteAngle::from_degrees(query.pose.pitch),
    };

    local
        .append(
            &play::PlayerPositionLookS2c {
                position: query.pose.position.as_dvec3(),
                yaw: query.pose.yaw,
                pitch: query.pose.pitch,
                flags: PlayerPositionLookFlags::default(),
                teleport_id: 1.into(),
            },
            &mut io,
        )
        .unwrap();

    // broadcast.append(&spawn_player, &mut io).unwrap();

    info!("Player {} joined the world", query.name);
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

pub fn send_keep_alive(packets: &mut Packets, io: &mut IoBuf) -> anyhow::Result<()> {
    let pkt = play::KeepAliveS2c {
        // The ID can be set to zero because it doesn't matter
        id: 0,
    };

    packets.append(&pkt, io)?;

    Ok(())
}

fn registry_codec_raw(codec: &RegistryCodec) -> anyhow::Result<Compound> {
    // codec.cached_codec.clear();

    let mut compound = Compound::default();

    for (reg_name, reg) in &codec.registries {
        let mut value = vec![];

        for (id, v) in reg.iter().enumerate() {
            let id = i32::try_from(id).context("id too large")?;
            value.push(compound! {
                "id" => id,
                "name" => v.name.as_str(),
                "element" => v.element.clone(),
            });
        }

        let registry = compound! {
            "type" => reg_name.as_str(),
            "value" => List::Compound(value),
        };

        compound.insert(reg_name.as_str(), registry);
    }

    Ok(compound)
}

pub fn send_game_join_packet(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    // recv ack

    let codec = RegistryCodec::default();

    let registry_codec = registry_codec_raw(&codec)?;

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
        game_mode: GameMode::Survival,
        is_flat: false,
        last_death_location: None,
        portal_cooldown: 60.into(),
        previous_game_mode: OptGameMode(Some(GameMode::Survival)),
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

    // TODO: Do we need to send this else where?
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

    for x in -16..16 {
        for z in -16..16 {
            pkt.pos = ChunkPos::new(x, z);
            encoder.append_packet(&pkt)?;
        }
    }

    send_commands(encoder)?;

    encoder.append_packet(&play::PlayerSpawnPositionS2c {
        position: BlockPos::default(),
        angle: 3.0,
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
