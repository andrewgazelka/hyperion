use std::{
    borrow::Cow,
    fs::File,
    io::{BufRead, BufReader},
    sync::Arc,
};

use anyhow::Context;
use base64::{engine::general_purpose, Engine};
use flecs_ecs::{
    core::{
        flecs,
        flecs::pipeline::{OnUpdate, PreUpdate},
        EntityView, IdOperations, Query, QueryAPI, QueryBuilderImpl, SystemAPI, TermBuilderImpl,
        World,
    },
    macros::Component,
    prelude::WorldRef,
};
use parking_lot::Mutex;
use serde_json::json;
use sha2::Digest;
use tracing::{error, instrument, trace, trace_span, warn};
use valence_protocol::{
    decode::PacketFrame,
    packets,
    packets::{
        handshaking::handshake_c2s::HandshakeNextState, login, login::LoginCompressionS2c, play,
    },
    Bounded, ByteAngle, Packet, VarInt,
};

use crate::{
    component,
    component::{
        blocks::Blocks, AiTargetable, ChunkPosition, Comms, EntityReaction, Health, ImmuneStatus,
        InGameName, PacketState, Pose, Uuid, PLAYER_SPAWN_POSITION,
    },
    net::{
        proxy::ReceiveStateInner, Compose, NetworkStreamRef, PacketDecoder, MINECRAFT_VERSION,
        PROTOCOL_VERSION,
    },
    packets::PacketSwitchQuery,
    runtime::AsyncRuntime,
    singleton::{fd_lookup::StreamLookup, player_id_lookup::EntityIdLookup},
    system::chunks::ChunkChanges,
    util::{db::SkinCollection, mojang::MojangClient, player_skin::PlayerSkin},
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
        .system_named::<&mut StreamLookup>("generate_ingress_events")
        .immediate(true)
        .kind::<PreUpdate>()
        .term_at(0)
        .singleton()
        // .multi_threaded(true)
        .each_iter(move |it, _, lookup| {
            let world = it.world();

            let span = tracing::info_span!("generate_ingress_events");
            let _enter = span.enter();
            let mut recv = shared.lock();

            for connect in recv.player_connect.drain(..) {
                let view = world
                    .entity()
                    .set(NetworkStreamRef::new(connect.stream))
                    .set(PacketState::Handshake)
                    .set(PacketDecoder::default())
                    .add::<component::Player>();

                lookup.insert(connect.stream, view.id());
            }

            for disconnect in recv.player_disconnect.drain(..) {
                // will initiate the removal of entity
                let id = lookup.get(&disconnect.stream).as_deref().copied().unwrap();
                world.entity_from_id(*id).add::<PendingRemove>();
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

                if !world.is_alive(*entity_id) {
                    continue;
                }

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
            &Compose,        // 0
            &EntityIdLookup, // 1
            &Blocks,         // 2
            &AsyncRuntime,   // 3
            &Comms,          // 4
            &SkinCollection, // 5
            &mut PacketDecoder,
            &mut PacketState,
            &NetworkStreamRef,
            Option<&mut Pose>,
            Option<&InGameName>,
        )>("recv_data")
        .kind::<OnUpdate>()
        // .multi_threaded()
        .term_at(0)
        .singleton()
        .term_at(1)
        .singleton()
        .term_at(2)
        .singleton()
        .term_at(3)
        .singleton()
        .term_at(4)
        .singleton()
        .term_at(5)
        .singleton()
        .each_iter(
            move |iter,
                  idx,
                  (
                compose,
                lookup,
                blocks,
                tasks,
                comms,
                skins_collection,
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

                loop {
                    let Some(frame) = decoder
                        .try_next_packet(&mut *compose.scratch().borrow_mut())
                        .unwrap()
                    else {
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
                            process_status(login_state, &frame, io_ref, compose).unwrap();
                        }
                        PacketState::Login => process_login(
                            &world,
                            lookup,
                            tasks,
                            login_state,
                            decoder,
                            comms,
                            skins_collection.clone(),
                            &frame,
                            io_ref,
                            compose,
                            &mut entity,
                            &query,
                        )
                        .unwrap(),
                        PacketState::Play => {
                            // We call this code when you're in play.
                            // Transitioning to play is just a way to make sure that the player is officially in play before we start sending them play packets.
                            // We have a certain duration that we wait before doing this.
                            // todo: better way?
                            if let Some(pose) = &mut pose {
                                let mut query = PacketSwitchQuery {
                                    id: entity.id(),
                                    view: entity,
                                    compose,
                                    io_ref,
                                    pose,
                                };

                                let name = name.map_or("unknown", |name| &***name);

                                tracing::info_span!("ingress", ign = name).in_scope(|| {
                                    if let Err(err) = crate::packets::packet_switch(
                                        &frame, &world, lookup, &mut query, blocks,
                                    ) {
                                        error!("failed to process packet {:?}: {err}", frame);
                                    }
                                });
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

fn process_handshake(login_state: &mut PacketState, packet: &PacketFrame) -> anyhow::Result<()> {
    debug_assert!(*login_state == PacketState::Handshake);

    let handshake: packets::handshaking::HandshakeC2s = packet.decode()?;

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
    world: &WorldRef,
    lookup: &EntityIdLookup,
    tasks: &AsyncRuntime,
    login_state: &mut PacketState,
    decoder: &mut PacketDecoder,
    comms: &Comms,
    skins_collection: SkinCollection,
    packet: &PacketFrame,
    stream_id: &NetworkStreamRef,
    compose: &Compose,
    entity: &mut EntityView,
    query: &Query<(&Uuid, &InGameName, &Pose)>,
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

    let global = compose.global();

    let pkt = LoginCompressionS2c {
        threshold: VarInt(global.shared.compression_threshold.0),
    };

    compose.unicast_no_compression(&pkt, stream_id).unwrap();

    decoder.set_compression(global.shared.compression_threshold);

    let pose = Pose::player(PLAYER_SPAWN_POSITION);
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

    compose.unicast(&pkt, stream_id).unwrap();

    // todo: impl rest
    *login_state = PacketState::Play;

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
        .set(Health::default())
        .set(Pose::player(PLAYER_SPAWN_POSITION))
        .set(ChunkChanges::default())
        .set(ChunkPosition::null())
        .set(EntityReaction::default());

    compose.io_buf().set_receive_broadcasts(stream_id);

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
    packet: &PacketFrame,
    packets: &NetworkStreamRef,
    compose: &Compose,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == PacketState::Status);

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
            *login_state = PacketState::Terminate;
        }

        _ => panic!("unexpected packet id: {}", packet.id),
    }

    // todo: check version is correct

    Ok(())
}
