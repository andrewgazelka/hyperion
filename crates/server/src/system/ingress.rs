use std::{borrow::Cow, sync::Arc};

use anyhow::Context;
use base64::{engine::general_purpose, Engine};
use flecs_ecs::{
    core::{
        flecs,
        flecs::pipeline::{OnUpdate, PreUpdate},
        EntityView, IdOperations, IterAPI, Query, QueryBuilderImpl, ReactorAPI, TermBuilderImpl,
        World,
    },
    macros::Component,
    prelude::WorldRef,
};
use parking_lot::Mutex;
use serde_json::json;
use sha2::Digest;
use tracing::{error, info, instrument, trace, trace_span, warn};
use valence_protocol::{
    decode::PacketFrame,
    packets,
    packets::{
        handshaking::handshake_c2s::HandshakeNextState, login, login::LoginCompressionS2c, play,
    },
    Bounded, ByteAngle, Packet, VarInt,
};
use valence_server::entity::EntityKind;

use crate::{
    component,
    component::{
        chunks::Blocks, AiTargetable, ChunkPosition, DisplaySkin, EntityReaction, Health,
        ImmuneStatus, InGameName, KeepAlive, LoginState, Pose, Uuid, Vitals, PLAYER_SPAWN_POSITION,
    },
    net::{
        proxy::ReceiveStateInner, Compose, IoRef, PacketDecoder, MINECRAFT_VERSION,
        PROTOCOL_VERSION,
    },
    packets::PacketSwitchQuery,
    singleton::{fd_lookup::StreamLookup, player_id_lookup::EntityIdLookup},
    system::{chunks::ChunkChanges, player_join_world::player_join_world},
    tasks::Tasks,
};

// pub type ThreadLocalIngressSender<'a, 'b> = SenderLocal<'a, 'b, IngressEventSet>;
// pub type IngressSender<'a> = Sender<'a, IngressEventSet>;

#[derive(Component, Debug)]
pub struct PendingRemove;

#[allow(
    clippy::significant_drop_in_scrutinee,
    reason = "I think this is fine. However, let's double check in perf"
)]
pub fn player_connect_disconnect(world: &World, shared: Arc<Mutex<ReceiveStateInner>>) {
    world
        .system_named::<(&mut StreamLookup, &Compose)>("generate_ingress_events")
        .immediate(true)
        .kind::<PreUpdate>()
        .term_at(0)
        .singleton()
        .term_at(1)
        .singleton()
        // .multi_threaded(true)
        .each_iter(move |it, _, (lookup, compose)| {
            let world = it.world();

            let span = tracing::info_span!("generate_ingress_events");
            let _enter = span.enter();
            let mut recv = shared.lock();

            for connect in recv.player_connect.drain(..) {
                let view = world
                    .entity()
                    .set(IoRef::new(connect.stream))
                    .set(LoginState::Handshake)
                    .set(PacketDecoder::default())
                    .add::<component::Player>();

                lookup.insert(connect.stream, view.id());
            }

            for disconnect in recv.player_disconnect.drain(..) {
                // will initiate the removal of entity
                let id = lookup.get(&disconnect.stream).as_deref().copied().unwrap();
                world.entity_from_id(*id).add::<PendingRemove>();

                let global = compose.global();

                global
                    .shared
                    .player_count
                    .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            }
        });
}

pub fn ingress_to_ecs(world: &World, shared: Arc<Mutex<ReceiveStateInner>>) {
    world
        .system_named::<&mut StreamLookup>("player_packets")
        .immediate(true)
        .kind::<OnUpdate>()
        .term_at(0)
        .singleton()
        // .multi_threaded(true)
        .each_iter(move |it, _, lookup| {
            let span = tracing::info_span!("player_packets");
            let _enter = span.enter();

            let mut recv = shared.lock();

            let world = it.world();

            for (entity_id, bytes) in recv.packets.drain() {
                let Some(entity_id) = lookup.get(&entity_id) else {
                    // this is not necessarily a bug; race conditions occur
                    warn!("player_packets: entity for {entity_id:?}");
                    continue;
                };

                let entity = world.entity_from_id(*entity_id);

                entity.get::<&mut PacketDecoder>(|decoder| {
                    decoder.queue_slice(bytes.as_ref());
                });
            }
        });
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn remove_player(world: &World) {
    world
        .observer::<flecs::OnAdd, (&PendingRemove, &Uuid, &Compose)>()
        .term_at(1)
        .filter()
        .term_at(2)
        .singleton()
        .each_entity(|entity, (_, uuid, compose)| {
            let uuids = &[uuid.0];
            let entity_ids = [VarInt(entity.id().0 as i32)];

            // destroy
            let pkt = play::EntitiesDestroyS2c {
                entity_ids: Cow::Borrowed(&entity_ids),
            };

            compose.broadcast(&pkt).send().unwrap();

            let pkt = play::PlayerRemoveS2c {
                uuids: Cow::Borrowed(uuids),
            };

            compose.broadcast(&pkt).send().unwrap();
            entity.destruct();
        });
}

#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn recv_data(world: &World) {
    let query = world.new_query::<(&Uuid, &InGameName, &Pose)>();

    world
        .system_named::<(
            &Compose,
            &EntityIdLookup,
            &Blocks,
            &Tasks,
            &mut PacketDecoder,
            &mut LoginState,
            &IoRef,
            Option<&mut Pose>,
            Option<&InGameName>,
        )>("recv_data")
        .kind::<OnUpdate>()
        .multi_threaded(true)
        .term_at(0)
        .singleton()
        .term_at(1)
        .singleton()
        .term_at(2)
        .singleton()
        .term_at(3)
        .singleton()
        .each_iter(
            move |iter,
                  idx,
                  (
                compose,
                lookup,
                blocks,
                tasks,
                decoder,
                login_state,
                io_ref,
                mut pose,
                name,
            )| {
                let span = trace_span!("recv_data");
                let _enter = span.enter();

                let mut entity = iter.entity(idx);

                let world = iter.world();
                let world = &world;

                loop {
                    let Some(frame) = decoder
                        .try_next_packet(&mut *compose.scratch().borrow_mut())
                        .unwrap()
                    else {
                        break;
                    };

                    match *login_state {
                        LoginState::Handshake => process_handshake(login_state, &frame).unwrap(),
                        LoginState::Status => {
                            process_status(login_state, &frame, io_ref, compose).unwrap();
                        }
                        LoginState::Login => process_login(
                            world,
                            lookup,
                            tasks,
                            blocks,
                            login_state,
                            decoder,
                            &frame,
                            io_ref,
                            compose,
                            &mut entity,
                            &query,
                        )
                        .unwrap(),
                        LoginState::TransitioningPlay { .. } | LoginState::Play => {
                            if let LoginState::TransitioningPlay {
                                packets_to_transition,
                            } = login_state
                            {
                                if *packets_to_transition == 0 {
                                    *login_state = LoginState::Play;

                                    compose.io_buf().set_receive_broadcasts(io_ref);
                                } else {
                                    *packets_to_transition -= 1;
                                }
                            }

                            // We call this code when you're in play.
                            // Transitioning to play is just a way to make sure that the player is officially in play before we start sending them play packets.
                            // We have a certain duration that we wait before doing this.
                            // todo: better way?
                            if let Some(pose) = &mut pose {
                                let mut query = PacketSwitchQuery {
                                    id: entity.id(),
                                    compose,
                                    io_ref,
                                    pose,
                                };

                                let name = name.map_or("unknown", |name| &***name);

                                tracing::info_span!("ingress", ign = name).in_scope(|| {
                                    if let Err(err) = crate::packets::packet_switch(
                                        &frame, world, lookup, &mut query, blocks,
                                    ) {
                                        error!("failed to process packet {:?}: {err}", frame);
                                    }
                                });
                            }
                        }
                        LoginState::Terminate => {
                            // todo
                        }
                    }
                }
            },
        );
}

fn process_handshake(login_state: &mut LoginState, packet: &PacketFrame) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Handshake);

    let handshake: packets::handshaking::HandshakeC2s = packet.decode()?;

    info!("received handshake: {:?}", handshake);

    // todo: check version is correct

    match handshake.next_state {
        HandshakeNextState::Status => {
            *login_state = LoginState::Status;
        }
        HandshakeNextState::Login => {
            *login_state = LoginState::Login;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "todo del")]
fn process_login(
    world: &WorldRef,
    lookup: &EntityIdLookup,
    tasks: &Tasks,
    blocks: &Blocks,
    login_state: &mut LoginState,
    decoder: &mut PacketDecoder,
    packet: &PacketFrame,
    stream_id: &IoRef,
    compose: &Compose,
    entity: &mut EntityView,
    query: &Query<(&Uuid, &InGameName, &Pose)>,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Login);

    let login::LoginHelloC2s { username, .. } = packet.decode()?;

    info!("received LoginHello for {username}");

    let username = username.0;

    let global = compose.global();

    let pkt = LoginCompressionS2c {
        threshold: VarInt(global.shared.compression_threshold.0),
    };

    compose.unicast_no_compression(&pkt, stream_id).unwrap();

    decoder.set_compression(global.shared.compression_threshold);

    let pose = Pose::player(PLAYER_SPAWN_POSITION);
    let username = Box::from(username);

    let uuid = offline_uuid(&username).unwrap();

    let pkt = login::LoginSuccessS2c {
        uuid,
        username: Bounded(&username),
        properties: Cow::default(),
    };

    compose.unicast(&pkt, stream_id).unwrap();

    // todo: impl rest
    *login_state = LoginState::TransitioningPlay {
        packets_to_transition: 5,
    };

    info!("enqueuing player join world");

    player_join_world(
        entity, tasks, blocks, compose, uuid, &username, stream_id, &pose, world, query,
    );

    // todo: this is jank and might break in certain cases
    lookup.insert(entity.id().0 as i32, entity.id());

    query
        .iter_stage(world)
        .each_iter(|it, idx, (uuid, _, pose)| {
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

            compose.unicast(&pkt, stream_id).unwrap();
        });

    entity
        .set(pose)
        .set(InGameName::from(username))
        .add::<AiTargetable>()
        .set(ImmuneStatus::default())
        .set(Uuid::from(uuid))
        // .set(PositionSyncMetadata::default())
        .set(KeepAlive::default())
        // .set(Prev::from(Vitals::ALIVE))
        .set(Vitals::ALIVE)
        // .set(PlayerInventory::new())
        .set(DisplaySkin(EntityKind::PLAYER))
        .set(Health::default())
        .set(Pose::player(PLAYER_SPAWN_POSITION))
        .set(ChunkChanges::default())
        .set(ChunkPosition::null())
        .set(EntityReaction::default());

    // world
    //     .event()
    //     .add::<flecs::Any>()
    //     .target(entity)
    //     .enqueue(&PlayerJoinWorld);

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
    login_state: &mut LoginState,
    packet: &PacketFrame,
    packets: &IoRef,
    compose: &Compose,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Status);

    info!("process status");

    match packet.id {
        packets::status::QueryRequestC2s::ID => {
            let query_request: packets::status::QueryRequestC2s = packet.decode()?;

            let img_bytes = include_bytes!("hyperion.png");
            // to base74

            let favicon = general_purpose::STANDARD.encode(img_bytes);
            let favicon = format!("data:image/png;base64,{favicon}");

            // https://wiki.vg/Server_List_Ping#Response
            let json = json!({
                "version": {
                    "name": MINECRAFT_VERSION,
                    "protocol": PROTOCOL_VERSION,
                },
                "players": {
                    "online": 1,
                    "max": 12_000,
                    "sample": [],
                },
                "description": "Getting 10k Players to PvP at Once on a Minecraft Server to Break the Guinness World Record",
                "favicon": favicon,
            });

            let json = serde_json::to_string_pretty(&json)?;

            let send = packets::status::QueryResponseS2c { json: &json };

            trace!("sent query response: {query_request:?}");
            compose.unicast_no_compression(&send, packets).unwrap();
        }

        packets::status::QueryPingC2s::ID => {
            let query_ping: packets::status::QueryPingC2s = packet.decode()?;

            let payload = query_ping.payload;

            let send = packets::status::QueryPongS2c { payload };

            compose.unicast_no_compression(&send, packets).unwrap();

            trace!("sent query pong: {query_ping:?}");
            *login_state = LoginState::Terminate;
        }

        _ => panic!("unexpected packet id: {}", packet.id),
    }

    // todo: check version is correct

    Ok(())
}
