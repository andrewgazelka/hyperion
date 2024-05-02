use std::{borrow::Cow, collections::BTreeSet};

use anyhow::{bail, Context};
use evenio::prelude::*;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::Deserialize;
use tracing::{debug, info, instrument, trace, warn};
use valence_nbt::{value::ValueRef, Value};
use valence_protocol::{
    game_mode::OptGameMode,
    ident,
    nbt::Compound,
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
    ByteAngle, ChunkPos, GameMode, Ident, ItemKind, ItemStack, PacketEncoder, VarInt,
};
use valence_registry::{
    biome::{Biome, BiomeEffects},
    BiomeRegistry, RegistryCodec,
};

use crate::{
    components::{
        chunks::Chunks, Display, FullEntityPose, InGameName, Player, Uuid, PLAYER_SPAWN_POSITION,
    },
    config::CONFIG,
    event::PlayerJoinWorld,
    global::Global,
    net::{Broadcast, Compose, Packets},
    singleton::{player_id_lookup::EntityIdLookup, player_uuid_lookup::PlayerUuidLookup},
    system::init_entity::spawn_entity_packet,
};

#[derive(Query, Debug)]
pub(crate) struct EntityQuery<'a> {
    id: EntityId,
    uuid: &'a Uuid,
    pose: &'a FullEntityPose,
    skin: &'a Display,
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
#[instrument(skip_all, level = "trace")]
pub fn player_join_world(
    r: Receiver<PlayerJoinWorld, PlayerJoinWorldQuery>,
    entities: Fetcher<EntityQuery>,
    global: Single<&Global>,
    players: Fetcher<PlayerQuery>,
    mut uuid_lookup: Single<&mut PlayerUuidLookup>,
    mut id_lookup: Single<&mut EntityIdLookup>,
    broadcast: Single<&Broadcast>,
    chunks: Single<&Chunks>,
    compose: Compose,
) {
    static CACHED_DATA: once_cell::sync::OnceCell<bytes::Bytes> = once_cell::sync::OnceCell::new();

    let compression_level = global.0.shared.compression_threshold;

    let cached_data = CACHED_DATA.get_or_init(|| {
        let mut encoder = PacketEncoder::new();
        encoder.set_compression(compression_level);

        info!("caching world data for new players");
        inner(&mut encoder, &chunks, &compose).unwrap();

        let bytes = encoder.take();
        bytes.freeze()
    });

    trace!("got cached data");

    let query = r.query;

    uuid_lookup.insert(query.uuid.0, query.id);
    id_lookup.insert(query.id.index().0 as i32, query.id);

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
        game_mode: GameMode::Adventure,
        display_name: Some(query.name.to_string().into_cow_text()),
    }];

    let current_entity_id = VarInt(query.id.index().0 as i32);

    let text = play::GameMessageS2c {
        chat: format!("{} joined the world", query.name).into_cow_text(),
        overlay: false,
    };

    broadcast.append(&text, &compose).unwrap();

    let local = query.packets;
    {
        local.append_raw(cached_data);
    }

    trace!("appending cached data");

    local
        .append(
            &crate::packets::vanilla::EntityEquipmentUpdateS2c {
                entity_id: VarInt(0),
                equipment: Cow::Borrowed(&equipment),
            },
            &compose,
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

    broadcast.append(&info, &compose).unwrap();

    for entity in entities {
        // todo: handle player?
        let pkt = spawn_entity_packet(entity.id, entity.skin.0, *entity.uuid, entity.pose);
        local.append(&pkt, &compose).unwrap();
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
            game_mode: GameMode::Adventure,
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
            &compose,
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
            &compose,
        )
        .unwrap();

    local
        .append(
            &play::PlayerListS2c {
                actions,
                entries: Cow::Owned(entries),
            },
            &compose,
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
            &compose,
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

        local.append(&pkt, &compose).unwrap();

        let pkt = crate::packets::vanilla::EntityEquipmentUpdateS2c {
            entity_id,
            equipment: Cow::Borrowed(&equipment),
        };
        local.append(&pkt, &compose).unwrap();
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
            &compose,
        )
        .unwrap();

    broadcast.append(&spawn_player, &compose).unwrap();

    broadcast
        .append(
            &crate::packets::vanilla::EntityEquipmentUpdateS2c {
                entity_id: current_entity_id,
                equipment: Cow::Borrowed(&equipment),
            },
            &compose,
        )
        .unwrap();

    info!("{} joined the world", query.name);
}

pub fn send_keep_alive(packets: &mut Packets, compose: &Compose) -> anyhow::Result<()> {
    let pkt = play::KeepAliveS2c {
        // The ID can be set to zero because it doesn't matter
        id: 0,
    };

    packets.append(&pkt, compose)?;

    Ok(())
}

fn registry_codec_raw() -> anyhow::Result<Compound> {
    let bytes = include_bytes!("paper-registry.json");
    let compound = serde_json::from_slice::<Compound>(bytes)?;
    Ok(compound)
}

pub fn generate_biome_registry() -> anyhow::Result<BiomeRegistry> {
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

    Ok(biome_registry)
}

pub fn send_game_join_packet(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    // recv ack

    let registry_codec = registry_codec_raw()?;
    let codec = RegistryCodec::default();

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
        max_players: CONFIG.max_players.into(),
        view_distance: CONFIG.view_distance.into(), // max view distance
        simulation_distance: CONFIG.simulation_distance.into(),
        reduced_debug_info: false,
        enable_respawn_screen: false,
        dimension_name: dimension_name.into(),
        hashed_seed: 0,
        game_mode: GameMode::Adventure,
        is_flat: false,
        last_death_location: None,
        portal_cooldown: 60.into(),
        previous_game_mode: OptGameMode(Some(GameMode::Adventure)),
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

fn send_sync_tags(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    let bytes = include_bytes!("tags.json");

    let groups = serde_json::from_slice(bytes)?;

    let pkt = play::SynchronizeTagsS2c { groups };

    encoder.append_packet(&pkt)?;

    Ok(())
}

fn inner(encoder: &mut PacketEncoder, chunks: &Chunks, compose: &Compose) -> anyhow::Result<()> {
    send_game_join_packet(encoder)?;
    send_sync_tags(encoder)?;

    let center_chunk = PLAYER_SPAWN_POSITION.as_ivec3() / 16;

    // TODO: Do we need to send this else where?
    encoder.append_packet(&play::ChunkRenderDistanceCenterS2c {
        chunk_x: center_chunk.x.into(),
        chunk_z: center_chunk.z.into(),
    })?;

    let radius = CONFIG.view_distance;

    // todo: right number?
    let number_chunks = (radius * 2 + 1) * (radius * 2 + 1);
    let bytes_to_append = crossbeam_queue::ArrayQueue::new(usize::try_from(number_chunks).unwrap());

    (0..number_chunks).into_par_iter().for_each(|i| {
        let x = i % (radius * 2 + 1);
        let z = i / (radius * 2 + 1);

        let x = center_chunk.x + x - radius;
        let z = center_chunk.z + z - radius;

        let chunk = ChunkPos::new(x, z);
        if let Ok(Some(chunk)) = chunks.get(chunk, compose) {
            bytes_to_append.push(chunk).unwrap();
        }
    });

    for elem in bytes_to_append {
        encoder.append_bytes(&elem);
    }

    send_commands(encoder)?;

    encoder.append_packet(&play::PlayerSpawnPositionS2c {
        position: PLAYER_SPAWN_POSITION.as_dvec3().into(),
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

    if let Some(diameter) = CONFIG.border_diameter {
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
