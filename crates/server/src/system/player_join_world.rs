use std::{borrow::Cow, collections::BTreeSet, io::Write};

use anyhow::{bail, Context};
use evenio::prelude::*;
use itertools::Itertools;
use libdeflater::Compressor;
use serde::Deserialize;
use tracing::{debug, error, info, instrument};
use valence_nbt::{value::ValueRef, Value};
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
use valence_registry::{
    biome::{Biome, BiomeEffects},
    BiomeRegistry, RegistryCodec, RegistryIdx,
};
use valence_server::layer::chunk::{bit_width, BiomeContainer, BlockStateContainer};

use crate::{
    bits::BitStorage,
    blocks::AnvilFolder,
    chunk::heightmap,
    components::{
        FullEntityPose, InGameName, MinecraftEntity, Player, Uuid, PLAYER_SPAWN_POSITION,
    },
    config,
    event::{PlayerJoinWorld, Scratch, ScratchBuffer},
    global::Global,
    net,
    net::{Broadcast, IoBuf, IoBufs, Packets},
    singleton::{player_id_lookup::EntityIdLookup, player_uuid_lookup::PlayerUuidLookup},
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
#[allow(clippy::too_many_arguments, reason = "todo")]
#[instrument(skip_all)]
pub fn player_join_world(
    r: Receiver<PlayerJoinWorld, PlayerJoinWorldQuery>,
    entities: Fetcher<EntityQuery>,
    global: Single<&Global>,
    players: Fetcher<PlayerQuery>,
    mut uuid_lookup: Single<&mut PlayerUuidLookup>,
    mut id_lookup: Single<&mut EntityIdLookup>,
    broadcast: Single<&Broadcast>,
    mut io: Single<&mut IoBufs>,
    mut compressor: Single<&mut net::Compressor>,
) {
    static CACHED_DATA: once_cell::sync::OnceCell<bytes::Bytes> = once_cell::sync::OnceCell::new();

    // todo: remove
    let mut scratch = Scratch::new();

    let compression_level = global.0.shared.compression_threshold;

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

    let compressor = compressor.one();

    let io = io.one();

    broadcast
        .append(&text, io, &mut scratch, compressor)
        .unwrap();

    let local = query.packets;

    local.append_raw(cached_data, io);

    info!("appending cached data");

    local
        .append(
            &crate::packets::vanilla::EntityEquipmentUpdateS2c {
                entity_id: VarInt(0),
                equipment: Cow::Borrowed(&equipment),
            },
            io,
            &mut scratch,
            compressor,
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

    broadcast
        .append(&info, io, &mut scratch, compressor)
        .unwrap();

    for entity in entities {
        let pkt = spawn_packet(entity.id, *entity.uuid, entity.pose);
        local.append(&pkt, io, &mut scratch, compressor).unwrap();
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
            io,
            &mut scratch,
            compressor,
        )
        .unwrap();

    let current_name = query.name;

    broadcast
        .append(
            &play::TeamS2c {
                team_name: "no_tag",
                mode: Mode::AddEntities {
                    entities: vec![current_name],
                },
            },
            io,
            &mut scratch,
            compressor,
        )
        .unwrap();

    local
        .append(
            &play::PlayerListS2c {
                actions,
                entries: Cow::Owned(entries),
            },
            io,
            &mut scratch,
            compressor,
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
            io,
            &mut scratch,
            compressor,
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

        local.append(&pkt, io, &mut scratch, compressor).unwrap();

        let pkt = crate::packets::vanilla::EntityEquipmentUpdateS2c {
            entity_id,
            equipment: Cow::Borrowed(&equipment),
        };
        local.append(&pkt, io, &mut scratch, compressor).unwrap();
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
            io,
            &mut scratch,
            compressor,
        )
        .unwrap();

    broadcast
        .append(&spawn_player, io, &mut scratch, compressor)
        .unwrap();

    broadcast
        .append(
            &crate::packets::vanilla::EntityEquipmentUpdateS2c {
                entity_id: current_entity_id,
                equipment: Cow::Borrowed(&equipment),
            },
            io,
            &mut scratch,
            compressor,
        )
        .unwrap();

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

    #[expect(dead_code, reason = "unused")]
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

pub fn send_keep_alive(
    packets: &Packets,
    io: &mut IoBuf,
    scratch: &mut impl ScratchBuffer,
    compressor: &mut Compressor,
) -> anyhow::Result<()> {
    let pkt = play::KeepAliveS2c {
        // The ID can be set to zero because it doesn't matter
        id: 0,
    };

    packets.append(&pkt, io, scratch, compressor)?;

    Ok(())
}

fn registry_codec_raw() -> anyhow::Result<Compound> {
    let bytes = include_bytes!("paper-registry.json");
    let compound = serde_json::from_slice::<Compound>(bytes)?;
    Ok(compound)
}

pub fn send_game_join_packet(encoder: &mut PacketEncoder) -> anyhow::Result<BiomeRegistry> {
    // recv ack

    let codec = RegistryCodec::default();

    let registry_codec = registry_codec_raw()?;

    // minecraft:worldgen/biome
    let biomes = registry_codec.get("minecraft:worldgen/biome").unwrap();

    let Value::Compound(biomes) = biomes else {
        bail!("expected biome to be compound");
    };

    let biomes = biomes
        .get("value")
        .context("expected biome to have value")?;

    let Value::List(biomes) = biomes else {
        bail!("expected biomes to be list");
    };

    let mut biome_registry = BiomeRegistry::default();

    for biome in biomes {
        let ValueRef::Compound(biome) = biome else {
            bail!("expected biome to be compound");
        };

        // let id = biome.get("id").context("expected biome to have id")?;
        // let Value::Int(id) = id else {
        //     bail!("expected biome id to be int");
        // };

        // let id = BiomeId::from_index(*id as usize);

        let name = biome.get("name").context("expected biome to have name")?;
        let Value::String(name) = name else {
            bail!("expected biome name to be string");
        };

        let biome = biome
            .get("element")
            .context("expected biome to have element")?;

        let Value::Compound(biome) = biome else {
            bail!("expected biome to be compound");
        };

        let biome = biome.clone();

        let downfall = biome
            .get("downfall")
            .context("expected biome to have downfall")?;
        let Value::Double(downfall) = downfall else {
            bail!("expected biome downfall to be float but is {downfall:?}");
        };

        let effects = biome
            .get("effects")
            .context("expected biome to have effects")?;
        let Value::Compound(effects) = effects else {
            bail!("expected biome effects to be compound but is {effects:?}");
        };

        let has_precipitation = biome.get("has_precipitation").with_context(|| {
            format!("expected biome biome for {name} to have has_precipitation")
        })?;
        let Value::Long(has_precipitation) = has_precipitation else {
            bail!("expected biome biome has_precipitation to be int but is {has_precipitation:?}");
        };
        let has_precipitation = *has_precipitation == 1;

        let temperature = biome
            .get("temperature")
            .context("expected biome to have temperature")?;
        let Value::Double(temperature) = temperature else {
            bail!("expected biome temperature to be doule but is {temperature:?}");
        };

        let effects = BiomeEffects::deserialize(effects.clone())?;

        let biome = Biome {
            downfall: *downfall as f32,
            effects,
            has_precipitation,
            temperature: *temperature as f32,
        };

        let ident = Ident::new(name.as_str()).unwrap();

        biome_registry.insert(ident, biome);
    }

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

    Ok(biome_registry)
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

fn encode_chunk_packet(
    anvil_folder: &mut AnvilFolder,
    location: ChunkPos,
    encoder: &mut PacketEncoder,
) -> anyhow::Result<()> {
    let chunk = anvil_folder.dim.get_chunk(location);

    let Ok(chunk) = chunk else {
        error!("failed to get chunk at {location:?}");
        return Ok(());
    };

    let Some(chunk) = chunk else {
        return Ok(());
    };

    let section_count = 384 / 16_usize;
    let chunk = chunk.chunk;
    let dimension_height = 384;

    let map = heightmap(dimension_height, dimension_height - 3);
    let map: Vec<_> = map.into_iter().map(i64::try_from).try_collect()?;

    // convert section_count + 2 0b1s into u64 array
    let mut bits = BitStorage::new(1, section_count + 2, None).unwrap();

    for i in 0..section_count + 2 {
        bits.set(i, 1);
    }

    // 2048 bytes per section -> long count = 2048 / 8 = 256
    let sky_light_array = FixedArray([0xFF_u8; 2048]);
    let sky_light_arrays = vec![sky_light_array; section_count + 2];

    let mut section_bytes = Vec::new();

    for section in chunk.sections {
        let non_air_blocks: u16 = 42;
        non_air_blocks.encode(&mut section_bytes).unwrap();

        write_block_states(&section.block_states, &mut section_bytes).unwrap();
        write_biomes(&section.biomes, &mut section_bytes).unwrap();
    }

    let pkt = play::ChunkDataS2c {
        pos: location,
        heightmaps: Cow::Owned(compound! {
            "MOTION_BLOCKING" => List::Long(map),
        }),
        blocks_and_biomes: &section_bytes,
        block_entities: Cow::Borrowed(&[]),

        sky_light_mask: Cow::Owned(bits.into_data()),
        block_light_mask: Cow::Borrowed(&[]),
        empty_sky_light_mask: Cow::Borrowed(&[]),
        empty_block_light_mask: Cow::Borrowed(&[]),
        sky_light_arrays: Cow::Owned(sky_light_arrays),
        block_light_arrays: Cow::Borrowed(&[]),
    };

    encoder.append_packet(&pkt)?;

    Ok(())
}

fn send_sync_tags(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    let bytes = include_bytes!("tags.json");

    let groups = serde_json::from_slice(bytes)?;

    let pkt = play::SynchronizeTagsS2c { groups };

    encoder.append_packet(&pkt)?;

    Ok(())
}

fn inner(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    let biome_registry = send_game_join_packet(encoder)?;
    send_sync_tags(encoder)?;

    let center_chunk = PLAYER_SPAWN_POSITION.as_ivec3() / 16;

    // TODO: Do we need to send this else where?
    encoder.append_packet(&play::ChunkRenderDistanceCenterS2c {
        chunk_x: center_chunk.x.into(),
        chunk_z: center_chunk.z.into(),
    })?;

    let mut anvil = AnvilFolder::new(&biome_registry).context("failed to get anvil data")?;

    for x in -16..16 {
        for z in -16..16 {
            let x = center_chunk.x + x;
            let z = center_chunk.z + z;

            let pos = ChunkPos::new(x, z);
            encode_chunk_packet(&mut anvil, pos, encoder)?;
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
            x: f64::from(PLAYER_SPAWN_POSITION.x),
            z: f64::from(PLAYER_SPAWN_POSITION.z),
            old_diameter: diameter,
            new_diameter: diameter,
            duration_millis: 1.into(),
            portal_teleport_boundary: 29_999_984.into(),
            warning_blocks: 50.into(),
            warning_time: 200.into(),
        })?;

        encoder.append_packet(&play::WorldBorderSizeChangedS2c { diameter })?;

        encoder.append_packet(&play::WorldBorderCenterChangedS2c {
            x_pos: f64::from(PLAYER_SPAWN_POSITION.x),
            z_pos: f64::from(PLAYER_SPAWN_POSITION.z),
        })?;
    }

    Ok(())
}
