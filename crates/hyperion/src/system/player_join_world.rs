use std::{borrow::Cow, collections::BTreeSet};

use anyhow::{bail, Context};
use flecs_ecs::{
    core::{Entity, IdOperations, Query, QueryAPI, World},
    prelude::{EntityView, WorldRef},
};
use glam::{I16Vec2, IVec3};
use serde::Deserialize;
use tracing::{debug, info, instrument};
use valence_nbt::{value::ValueRef, Value};
use valence_protocol::{
    game_mode::OptGameMode,
    ident,
    nbt::Compound,
    packets::{
        play,
        play::{
            player_position_look_s2c::PlayerPositionLookFlags,
            team_s2c::{CollisionRule, Mode, NameTagVisibility, TeamColor, TeamFlags},
            GameJoinS2c,
        },
    },
    ByteAngle, GameMode, Ident, PacketEncoder, RawBytes, VarInt, Velocity,
};
use valence_registry::{
    biome::{Biome, BiomeEffects},
    BiomeRegistry, RegistryCodec,
};
use valence_server::entity::EntityKind;
use valence_text::IntoText;

use crate::{
    component::{
        blocks::MinecraftWorld, command::get_command_packet, InGameName, Pose, Uuid,
        PLAYER_SPAWN_POSITION,
    },
    config::CONFIG,
    net::{Compose, NetworkStreamRef},
    runtime::AsyncRuntime,
    system::player_join_world::list::{PlayerListActions, PlayerListEntry, PlayerListS2c},
    util::{metadata::show_all, player_skin::PlayerSkin},
    SystemId,
};

pub mod list;

#[allow(clippy::too_many_arguments, reason = "todo")]
#[instrument(skip_all, fields(name = name))]
pub fn player_join_world(
    entity: &EntityView<'_>,
    tasks: &AsyncRuntime,
    chunks: &MinecraftWorld,
    compose: &Compose,
    uuid: uuid::Uuid,
    name: &str,
    packets: NetworkStreamRef,
    pose: &Pose,
    world: &WorldRef<'_>,
    skin: &PlayerSkin,
    system_id: SystemId,
    root_command: Entity,
    query: &Query<(&Uuid, &InGameName, &Pose, &PlayerSkin)>,
) {
    static CACHED_DATA: once_cell::sync::OnceCell<bytes::Bytes> = once_cell::sync::OnceCell::new();

    let cached_data = CACHED_DATA
        .get_or_init(|| {
            let compression_level = compose.global().shared.compression_threshold;
            let mut encoder = PacketEncoder::new();
            encoder.set_compression(compression_level);

            info!(
                "caching world data for new players with compression level {compression_level:?}"
            );
            inner(&mut encoder, chunks, tasks, world).unwrap();

            let bytes = encoder.take();
            bytes.freeze()
        })
        .clone();

    compose
        .io_buf()
        .unicast_raw(cached_data, packets, system_id, world);

    let text = play::GameMessageS2c {
        chat: format!("{name} joined the world").into_cow_text(),
        overlay: false,
    };

    compose.broadcast(&text, system_id).send(world).unwrap();

    compose
        .unicast(
            &play::PlayerPositionLookS2c {
                position: pose.position.as_dvec3(),
                yaw: pose.yaw,
                pitch: pose.pitch,
                flags: PlayerPositionLookFlags::default(),
                teleport_id: 1.into(),
            },
            packets,
            system_id,
            world,
        )
        .unwrap();

    let mut entries = Vec::new();
    let mut all_player_names = Vec::new();

    let count = query.iter_stage(world).count();

    info!("sending skins for {count} players");

    query.iter_stage(world).each(|(uuid, name, _, skin)| {
        info!("sending skin for {name}");
        // todo: in future, do not clone

        let PlayerSkin {
            textures: value,
            signature,
        } = skin.clone();

        let property = valence_protocol::profile::Property {
            name: "textures".to_string(),
            value,
            signature: Some(signature),
        };

        let entry = PlayerListEntry {
            player_uuid: uuid.0,
            username: name.to_string().into(),
            // todo: eliminate alloc
            properties: Cow::Owned(vec![property]),
            chat_data: None,
            listed: true,
            ping: 20,
            game_mode: GameMode::Creative,
            display_name: Some(name.to_string().into_cow_text()),
        };

        entries.push(entry);
        all_player_names.push(name.to_string());
    });

    let all_player_names = all_player_names.iter().map(String::as_str).collect();

    let actions = PlayerListActions::default()
        .with_add_player(true)
        .with_update_listed(true)
        .with_update_display_name(true);

    compose
        .unicast(
            &PlayerListS2c {
                actions,
                entries: Cow::Owned(entries),
            },
            packets,
            system_id,
            world,
        )
        .unwrap();

    query
        .iter_stage(world)
        .each_iter(|it, idx, (uuid, _, pose, _)| {
            let query_entity = it.entity(idx);

            if entity.id() == query_entity.id() {
                return;
            }

            let pkt = play::PlayerSpawnS2c {
                entity_id: VarInt(query_entity.id().0 as i32),
                player_uuid: uuid.0,
                position: pose.position.as_dvec3(),
                yaw: ByteAngle::from_degrees(pose.yaw),
                pitch: ByteAngle::from_degrees(pose.pitch),
            };

            compose.unicast(&pkt, packets, system_id, world).unwrap();

            let show_all = show_all(query_entity.id().0 as i32);
            compose
                .unicast(show_all.borrow_packet(), packets, system_id, world)
                .unwrap();
        });

    let PlayerSkin {
        textures,
        signature,
    } = skin.clone();

    // todo: in future, do not clone
    let property = valence_protocol::profile::Property {
        name: "textures".to_string(),
        value: textures,
        signature: Some(signature),
    };

    let property = &[property];

    let singleton_entry = &[PlayerListEntry {
        player_uuid: uuid,
        username: Cow::Borrowed(name),
        properties: Cow::Borrowed(property),
        chat_data: None,
        listed: true,
        ping: 20,
        game_mode: GameMode::Survival,
        display_name: Some(name.to_string().into_cow_text()),
    }];

    let pkt = PlayerListS2c {
        actions,
        entries: Cow::Borrowed(singleton_entry),
    };

    // todo: fix broadcasting on first tick; and this duplication can be removed!
    compose.broadcast(&pkt, system_id).send(world).unwrap();
    compose.unicast(&pkt, packets, system_id, world).unwrap();

    let player_name = vec![name];

    compose
        .broadcast(
            &play::TeamS2c {
                team_name: "no_tag",
                mode: Mode::AddEntities {
                    entities: player_name,
                },
            },
            system_id,
        )
        .exclude(packets)
        .send(world)
        .unwrap();

    let current_entity_id = VarInt(entity.id().0 as i32);

    let spawn_player = play::PlayerSpawnS2c {
        entity_id: current_entity_id,
        player_uuid: uuid,
        position: pose.position.as_dvec3(),
        yaw: ByteAngle::from_degrees(pose.yaw),
        pitch: ByteAngle::from_degrees(pose.pitch),
    };
    compose
        .broadcast(&spawn_player, system_id)
        .exclude(packets)
        .send(world)
        .unwrap();

    let show_all = show_all(entity.id().0 as i32);
    compose
        .broadcast(show_all.borrow_packet(), system_id)
        .exclude(packets)
        .send(world)
        .unwrap();

    compose
        .unicast(
            &play::TeamS2c {
                team_name: "no_tag",
                mode: Mode::AddEntities {
                    entities: all_player_names,
                },
            },
            packets,
            system_id,
            world,
        )
        .unwrap();

    let command_packet = get_command_packet(world, root_command);

    compose
        .unicast(&command_packet, packets, system_id, world)
        .unwrap();

    info!("{name} joined the world");
}

#[allow(dead_code, reason = "will re-enable")]
pub fn send_keep_alive(
    packets: NetworkStreamRef,
    compose: &Compose,
    system_id: SystemId,
    world: &World,
) -> anyhow::Result<()> {
    let pkt = play::KeepAliveS2c {
        // The ID can be set to zero because it doesn't matter
        id: 0,
    };

    compose.unicast(&pkt, packets, system_id, world)?;
    Ok(())
}

fn registry_codec_raw() -> anyhow::Result<Compound> {
    let bytes = include_bytes!("registries.nbt");
    let mut bytes = &bytes[..];
    let bytes_reader = &mut bytes;
    let (compound, _) = valence_nbt::from_binary(bytes_reader)?;
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
        let Value::Float(downfall) = downfall else {
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
        let Value::Byte(has_precipitation) = has_precipitation else {
            bail!("expected biome biome has_precipitation to be byte but is {has_precipitation:?}");
        };
        let has_precipitation = *has_precipitation == 1;

        let temperature = biome
            .get("temperature")
            .context("expected biome to have temperature")?;
        let Value::Float(temperature) = temperature else {
            bail!("expected biome temperature to be doule but is {temperature:?}");
        };

        let effects = BiomeEffects::deserialize(effects.clone())?;

        let biome = Biome {
            downfall: *downfall,
            effects,
            has_precipitation,
            temperature: *temperature,
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

    let dimension_names: BTreeSet<Ident<Cow<'_, str>>> = codec
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

fn send_sync_tags(encoder: &mut PacketEncoder) -> anyhow::Result<()> {
    let bytes = include_bytes!("tags.json");

    let groups = serde_json::from_slice(bytes)?;

    let pkt = play::SynchronizeTagsS2c { groups };

    encoder.append_packet(&pkt)?;

    Ok(())
}

fn inner(
    encoder: &mut PacketEncoder,
    chunks: &MinecraftWorld,
    tasks: &AsyncRuntime,
    world: &World,
) -> anyhow::Result<()> {
    send_game_join_packet(encoder)?;
    send_sync_tags(encoder)?;

    let mut buf: heapless::Vec<u8, 32> = heapless::Vec::new();
    let brand = b"discord: andrewgazelka";
    buf.push(brand.len() as u8).unwrap();
    buf.extend_from_slice(brand).unwrap();

    let bytes = RawBytes::from(buf.as_slice());

    let brand = play::CustomPayloadS2c {
        channel: ident!("minecraft:brand").into(),
        data: bytes.into(),
    };

    encoder.append_packet(&brand)?;

    let center_chunk: IVec3 = PLAYER_SPAWN_POSITION.as_ivec3() >> 4;

    // TODO: Do we need to send this else where?
    encoder.append_packet(&play::ChunkRenderDistanceCenterS2c {
        chunk_x: center_chunk.x.into(),
        chunk_z: center_chunk.z.into(),
    })?;

    let center_chunk = I16Vec2::new(center_chunk.x as i16, center_chunk.z as i16);

    // so they do not fall
    let chunk = unsafe { chunks.get_and_wait(center_chunk, tasks, world) };
    encoder.append_bytes(&chunk);

    // let radius = 2;

    // todo: right number?
    // let number_chunks = (radius * 2 + 1) * (radius * 2 + 1);
    //
    // (0..number_chunks).into_par_iter().for_each(|i| {
    //     let x = i % (radius * 2 + 1);
    //     let z = i / (radius * 2 + 1);
    //
    //     let x = center_chunk.x + x - radius;
    //     let z = center_chunk.z + z - radius;
    //
    //     let chunk = ChunkPos::new(x, z);
    //     if let Ok(Some(chunk)) = chunks.get(chunk, compose) {
    //         bytes_to_append.push(chunk).unwrap();
    //     }
    // });
    //
    // for elem in bytes_to_append {
    //     encoder.append_bytes(&elem);
    // }

    // send_commands(encoder)?;

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

    let show_all = show_all(0);
    encoder.append_packet(show_all.borrow_packet())?;

    Ok(())
}

#[tracing::instrument(skip_all)]
pub fn spawn_entity_packet(
    id: Entity,
    kind: EntityKind,
    uuid: Uuid,
    pose: &Pose,
) -> play::EntitySpawnS2c {
    info!("spawning entity");

    let entity_id = VarInt(id.0 as i32);

    play::EntitySpawnS2c {
        entity_id,
        object_uuid: *uuid,
        kind: VarInt(kind.get()),
        position: pose.position.as_dvec3(),
        pitch: ByteAngle::from_degrees(pose.pitch),
        yaw: ByteAngle::from_degrees(pose.yaw),
        head_yaw: ByteAngle::from_degrees(pose.head_yaw()),
        data: VarInt::default(),
        velocity: Velocity([0; 3]),
    }
}
