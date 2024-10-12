use std::{
    borrow::Cow,
    fs::File,
    io::{BufRead, BufReader},
};

use anyhow::Context;
use base64::{engine::general_purpose, Engine};
use flecs_ecs::prelude::*;
use parking_lot::Mutex;
use serde_json::json;
use sha2::Digest;
use tracing::{error, info, trace, trace_span, warn};
use valence_protocol::{
    packets,
    packets::{
        handshaking::handshake_c2s::HandshakeNextState, login, login::LoginCompressionS2c, play,
    },
    Bounded, Packet, VarInt,
};

use crate::{
    egress::sync_chunks::ChunkSendQueue,
    net::{
        decoder::BorrowedPacketFrame, proxy::ReceiveState, Compose, NetworkStreamRef,
        PacketDecoder, MINECRAFT_VERSION, PROTOCOL_VERSION,
    },
    runtime::AsyncRuntime,
    simulation::{
        animation::ActiveAnimation, blocks::Blocks, handlers::PacketSwitchQuery,
        metadata::Metadata, skin::PlayerSkin, AiTargetable, ChunkPosition, Comms,
        ConfirmBlockSequences, EntityReaction, Health, ImmuneStatus, InGameName, PacketState,
        Player, Position, StreamLookup, Uuid, PLAYER_SPAWN_POSITION,
    },
    storage::{Events, GlobalEventHandlers, PlayerJoinServer, SkinHandler},
    system_registry::{SystemId, RECV_DATA, REMOVE_PLAYER_FROM_VISIBILITY},
    util::{mojang::MojangClient, SendableRef, TracingExt},
};

#[derive(Component, Debug)]
pub struct PendingRemove;

fn process_handshake(
    login_state: &mut PacketState,
    packet: &BorrowedPacketFrame<'_>,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == PacketState::Handshake);

    let handshake: packets::handshaking::HandshakeC2s<'_> = packet.decode()?;

    // info!("received handshake: {:?}", handshake);

    // todo: check version is correct

    match handshake.next_state {
        HandshakeNextState::Status => {
            *login_state = PacketState::Status;
        }
        HandshakeNextState::Login => {
            *login_state = PacketState::Login;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "todo del")]
fn process_login(
    world: &WorldRef<'_>,
    tasks: &AsyncRuntime,
    login_state: &mut PacketState,
    decoder: &PacketDecoder,
    comms: &Comms,
    skins_collection: SkinHandler,
    packet: &BorrowedPacketFrame<'_>,
    stream_id: NetworkStreamRef,
    compose: &Compose,
    entity: &EntityView<'_>,
    system_id: SystemId,
    handlers: &GlobalEventHandlers,
    query: &Query<(&Uuid, &InGameName, &Position)>,
) -> anyhow::Result<()> {
    static UUIDS: once_cell::sync::Lazy<Mutex<Vec<uuid::Uuid>>> =
        once_cell::sync::Lazy::new(|| {
            let uuids = File::open("10000uuids.txt").unwrap();
            let uuids = BufReader::new(uuids);

            let uuids = uuids
                .lines()
                .map(|line| line.unwrap())
                .map(|line| uuid::Uuid::parse_str(&line).unwrap())
                .collect::<Vec<_>>();
            Mutex::new(uuids)
        });

    debug_assert!(*login_state == PacketState::Login);

    let login::LoginHelloC2s { username, .. } = packet.decode()?;

    let username = username.0;

    let mut player_join = PlayerJoinServer {
        username: username.to_string(),
        entity: entity.id(),
    };

    handlers.join_server.trigger_all(world, &mut player_join);

    let username = player_join.username.as_str();

    let global = compose.global();

    let pkt = LoginCompressionS2c {
        threshold: VarInt(global.shared.compression_threshold.0),
    };

    compose.unicast_no_compression(&pkt, stream_id, system_id, world)?;

    decoder.set_compression(global.shared.compression_threshold);

    let pose = Position::player(PLAYER_SPAWN_POSITION);
    let username = Box::from(username);

    let uuid = UUIDS
        .lock()
        .pop()
        .unwrap_or_else(|| offline_uuid(&username).unwrap());

    println!("{username} spawning with uuid {uuid:?}");

    let skins = comms.skins_tx.clone();
    let id = entity.id();
    tasks.spawn(async move {
        let mojang = MojangClient::default();
        let skin = PlayerSkin::from_uuid(uuid, &mojang, &skins_collection)
            .await
            .unwrap()
            .unwrap();

        skins.send((id, skin)).unwrap();
    });

    let pkt = login::LoginSuccessS2c {
        uuid,
        username: Bounded(&username),
        properties: Cow::default(),
    };

    compose.unicast(&pkt, stream_id, system_id, world).unwrap();

    *login_state = PacketState::Play;

    entity
        .set(pose)
        .set(InGameName::from(username))
        .add::<AiTargetable>()
        .set(ImmuneStatus::default())
        .set(Uuid::from(uuid))
        .set(Health::default())
        .set(Position::player(PLAYER_SPAWN_POSITION))
        .set(ChunkSendQueue::default())
        .set(ChunkPosition::null())
        .set(EntityReaction::default());

    compose.io_buf().set_receive_broadcasts(stream_id, world);

    Ok(())
}

/// Get a [`uuid::Uuid`] based on the given user's name.
fn offline_uuid(username: &str) -> anyhow::Result<uuid::Uuid> {
    let digest = sha2::Sha256::digest(username);

    #[expect(clippy::indexing_slicing, reason = "sha256 is always 32 bytes")]
    let slice = &digest[..16];

    uuid::Uuid::from_slice(slice).context("failed to create uuid")
}

fn process_status(
    login_state: &mut PacketState,
    system_id: SystemId,
    packet: &BorrowedPacketFrame<'_>,
    packets: NetworkStreamRef,
    compose: &Compose,
    world: &World,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == PacketState::Status);

    match packet.id {
        packets::status::QueryRequestC2s::ID => {
            let query_request: packets::status::QueryRequestC2s = packet.decode()?;

            let img_bytes = include_bytes!("data/hyperion.png");

            let favicon = general_purpose::STANDARD.encode(img_bytes);
            let favicon = format!("data:image/png;base64,{favicon}");

            let online = compose
                .global()
                .player_count
                .load(std::sync::atomic::Ordering::Relaxed);

            // https://wiki.vg/Server_List_Ping#Response
            let json = json!({
                "version": {
                    "name": MINECRAFT_VERSION,
                    "protocol": PROTOCOL_VERSION,
                },
                "players": {
                    "online": online,
                    "max": 12_000,
                    "sample": [],
                },
                "description": "Getting 10k Players to PvP at Once on a Minecraft Server to Break the Guinness World Record",
                "favicon": favicon,
            });

            let json = serde_json::to_string_pretty(&json)?;

            let send = packets::status::QueryResponseS2c { json: &json };

            trace!("sent query response: {query_request:?}");
            compose
                .unicast_no_compression(&send, packets, system_id, world)
                .unwrap();
        }

        packets::status::QueryPingC2s::ID => {
            let query_ping: packets::status::QueryPingC2s = packet.decode()?;

            let payload = query_ping.payload;

            let send = packets::status::QueryPongS2c { payload };

            compose
                .unicast_no_compression(&send, packets, system_id, world)
                .unwrap();

            trace!("sent query pong: {query_ping:?}");
            *login_state = PacketState::Terminate;
        }

        _ => panic!("unexpected packet id: {}", packet.id),
    }

    // todo: check version is correct

    Ok(())
}

#[derive(Component)]
pub struct IngressModule;

impl Module for IngressModule {
    #[allow(clippy::too_many_lines)]
    fn module(world: &World) {
        system!(
            "generate_ingress_events",
            world,
            &mut StreamLookup($),
            &ReceiveState($)
        )
        .immediate(true)
        .kind::<flecs::pipeline::OnLoad>()
        .term_at(0)
        .each_iter(move |it, _, (lookup, receive)| {
            let span = trace_span!("generate_ingress_events");
            let _enter = span.enter();

            let world = it.world();

            let mut recv = receive.0.lock();

            for connect in recv.player_connect.drain(..) {
                info!("player_connect");
                let view = world
                    .entity()
                    .set(NetworkStreamRef::new(connect.stream))
                    .set(hyperion_inventory::PlayerInventory::default())
                    .set(ConfirmBlockSequences::default())
                    .set(PacketState::Handshake)
                    .set(Metadata::default())
                    .set(ActiveAnimation::NONE)
                    .set(PacketDecoder::default())
                    .add::<Player>();

                lookup.insert(connect.stream, view.id());
            }

            for disconnect in recv.player_disconnect.drain(..) {
                // will initiate the removal of entity
                info!("queue pending remove");
                let id = lookup.get(&disconnect.stream).copied().unwrap();
                world.entity_from_id(*id).add::<PendingRemove>();
            }
        });

        let worlds = (0..rayon::current_num_threads() as i32)
            // SAFETY: promoting world to static lifetime, system won't outlive world
            .map(|i| unsafe { std::mem::transmute(world.stage(i)) })
            .map(SendableRef)
            .collect::<Vec<_>>();

        system!(
            "ingress_to_ecs",
            world,
            &StreamLookup($),
            &ReceiveState($),
        )
        .immediate(true)
        .kind::<flecs::pipeline::PostLoad>()
        .each(move |(lookup, receive)| {
            use rayon::prelude::*;

            // 134µs with par_iter
            // 150-208µs with regular drain
            let span = trace_span!("ingress_to_ecs");
            let _enter = span.enter();

            let mut recv = receive.0.lock();

            recv.packets.par_drain().for_each(|(entity_id, bytes)| {
                let world = &worlds[rayon::current_thread_index().unwrap_or_default()];
                let world = &world.0;

                let Some(entity_id) = lookup.get(&entity_id) else {
                    // this is not necessarily a bug; race conditions occur
                    warn!("player_packets: entity for {entity_id:?}");
                    return;
                };

                if !world.is_alive(*entity_id) {
                    return;
                }

                let entity = world.entity_from_id(*entity_id);

                entity.get::<&mut PacketDecoder>(|decoder| {
                    decoder.shift_excess();
                    decoder.queue_slice(bytes.as_ref());
                });
            });
        });

        let system_id = REMOVE_PLAYER_FROM_VISIBILITY;

        system!(
            "remove_player_from_visibility",
            world,
            &Uuid,
            &Compose($),
        )
        .with::<&PendingRemove>()
        .kind::<flecs::pipeline::PostLoad>()
        .tracing_each_entity(
            trace_span!("remove_player"),
            move |entity, (uuid, compose)| {
                let uuids = &[uuid.0];
                let entity_ids = [VarInt(entity.id().0 as i32)];

                let world = entity.world();

                // destroy
                let pkt = play::EntitiesDestroyS2c {
                    entity_ids: Cow::Borrowed(&entity_ids),
                };

                compose.broadcast(&pkt, system_id).send(&world).unwrap();

                let pkt = play::PlayerRemoveS2c {
                    uuids: Cow::Borrowed(uuids),
                };

                compose.broadcast(&pkt, system_id).send(&world).unwrap();
            },
        );

        world
            .system_named::<()>("remove_player")
            .kind::<flecs::pipeline::PostLoad>()
            .with::<&PendingRemove>()
            .tracing_each_entity(trace_span!("remove_player"), |entity, ()| {
                entity.destruct();
            });

        let query = query!(world, &Uuid, &InGameName, &Position).build();

        let system_id = RECV_DATA;

        system!(
            "recv_data",
            world,
            &Compose($),
            &Blocks($),
            &AsyncRuntime($),
            &Comms($),
            &SkinHandler($),
            &GlobalEventHandlers($),
            &mut PacketDecoder,
            &mut PacketState,
            &NetworkStreamRef,
            &Events($),
            ?&mut Position,
            &mut ConfirmBlockSequences,
            &mut hyperion_inventory::PlayerInventory,
            &mut Metadata,
            &mut ActiveAnimation,
            &hyperion_crafting::CraftingRegistry($),
        )
        .kind::<flecs::pipeline::OnUpdate>()
        .multi_threaded()
        .tracing_each_entity(
            trace_span!("recv_data"),
            move |entity,
                  (
                compose,
                blocks,
                tasks,
                comms,
                skins_collection,
                handlers,
                decoder,
                login_state,
                &io_ref,
                event_queue,
                mut pose,
                confirm_block_sequences,
                inventory,
                metadata,
                animation,
                crafting_registry,
            )| {
                let world = entity.world();
                let bump = compose.bump.get(&world);

                loop {
                    let Some(frame) = decoder.try_next_packet(bump).unwrap() else {
                        break;
                    };

                    match *login_state {
                        PacketState::Handshake => {
                            if process_handshake(login_state, &frame).is_err() {
                                error!("failed to process handshake");

                                entity.destruct();

                                break;
                            }
                        }
                        PacketState::Status => {
                            process_status(login_state, system_id, &frame, io_ref, compose, &world)
                                .unwrap();
                        }
                        PacketState::Login => process_login(
                            &world,
                            tasks,
                            login_state,
                            decoder,
                            comms,
                            skins_collection.clone(),
                            &frame,
                            io_ref,
                            compose,
                            &entity,
                            system_id,
                            handlers,
                            &query,
                        )
                        .unwrap(),
                        PacketState::Play => {
                            // We call this code when you're in play.
                            // Transitioning to play is just a way to make sure that the player is officially in play before we start sending them play packets.
                            // We have a certain duration that we wait before doing this.
                            // todo: better way?
                            if let Some(pose) = &mut pose {
                                let world = &world;

                                let mut query = PacketSwitchQuery {
                                    id: entity.id(),
                                    view: entity,
                                    compose,
                                    io_ref,
                                    pose,
                                    events: event_queue,
                                    world,
                                    blocks,
                                    system_id,
                                    confirm_block_sequences,
                                    inventory,
                                    metadata,
                                    animation,
                                    crafting_registry,
                                };

                                // trace_span!("ingress", ign = name).in_scope(|| {
                                if let Err(err) =
                                    crate::simulation::handlers::packet_switch(frame, &mut query)
                                {
                                    error!("failed to process packet {:?}: {err}", frame);
                                }
                                // });
                            }
                        }
                        PacketState::Terminate => {
                            // todo
                        }
                    }
                }
            },
        );
    }
}
