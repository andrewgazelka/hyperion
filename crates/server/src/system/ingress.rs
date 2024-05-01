use evenio::{
    event::{Despawn, Event, Insert, Receiver, Sender, Spawn},
    fetch::{Fetcher, Single},
    prelude::{EntityId, ReceiverMut},
    world::World,
};
use fxhash::FxHashMap;
use mio::unix::pipe::new;
use rayon_local::RayonLocal;
use serde_json::json;
use tracing::{info, instrument, trace, warn};
use valence_protocol::{
    decode::PacketFrame,
    packets,
    packets::{handshaking::handshake_c2s::HandshakeNextState, login, login::LoginCompressionS2c},
    Packet, VarInt,
};

use crate::{
    event,
    global::Global,
    net::{Server, ServerDef, ServerEvent},
    singleton::fd_lookup::FdLookup,
};

mod player_packet_buffer;

use rayon::iter::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};

use crate::{
    components::{FullEntityPose, ImmuneStatus, KeepAlive, LoginState, Vitals},
    event::BumpScratch,
    net::{
        buffers::BufferAllocator, Compose, Fd, Packets, Servers, MINECRAFT_VERSION,
        PROTOCOL_VERSION,
    },
    packets::PacketSwitchQuery,
    singleton::player_id_lookup::EntityIdLookup,
    system::ingress::player_packet_buffer::DecodeBuffer,
};
use crate::net::buffers::BufferAllocators;

pub type IngressSender<'a> = Sender<
    'a,
    (
        Spawn,
        Insert<LoginState>,
        Insert<DecodeBuffer>,
        Insert<Fd>,
        Insert<Packets>,
        Despawn,
        event::PlayerInit,
        event::KickPlayer,
        event::InitEntity,
        event::SwingArm,
        event::AttackEntity,
        event::BlockStartBreak,
        event::BlockAbortBreak,
        event::BlockFinishBreak,
        (event::Command, event::PoseUpdate),
    ),
>;

#[derive(Event)]
pub struct AddPlayers<'a> {
    fd: RayonLocal<Vec<Fd>>,
    servers: &'a mut Servers,
}

#[derive(Event)]
pub struct RemovePlayers<'a> {
    fd: RayonLocal<Vec<Fd>>,
    servers: &'a mut Servers,
}

// todo: do we really need three different lifetimes here?
#[derive(Event)]
pub struct RecvAllData {
    inner: RayonLocal<Vec<(Fd, &'static [u8])>>,
}

#[derive(Event)]
pub struct SentData {
    decrease_count: RayonLocal<FxHashMap<Fd, usize>>,
}

#[instrument(skip_all, level = "trace")]
pub fn generate_ingress_events(world: &mut World, servers: &mut Servers) {
    let decrease_count = RayonLocal::init(FxHashMap::default);
    let add_player = RayonLocal::init(Vec::new);
    let remove_player = RayonLocal::init(Vec::new);
    let recv_data = RayonLocal::init(Vec::new);

    rayon::broadcast(|_| {
        let server = servers.get_local_raw();
        let server = unsafe { &mut *server.get() };

        let decrease_count = decrease_count.get_local_raw();
        let decrease_count = unsafe { &mut *decrease_count.get() };

        let add_player = add_player.get_local_raw();
        let add_player = unsafe { &mut *add_player.get() };

        let remove_player = remove_player.get_local_raw();
        let remove_player = unsafe { &mut *remove_player.get() };

        let recv_data = recv_data.get_local_raw();
        let recv_data = unsafe { &mut *recv_data.get() };

        server
            .drain(|event| match event {
                ServerEvent::AddPlayer { fd } => {
                    add_player.push(fd);
                }
                ServerEvent::RemovePlayer { fd } => {
                    remove_player.push(fd);
                }
                ServerEvent::RecvData { fd, data } => {
                    recv_data.push((fd, data));
                }
                ServerEvent::SentData { fd } => {
                    decrease_count
                        .entry(fd)
                        .and_modify(|x| *x += 1)
                        .or_insert(1);
                }
            })
            .unwrap();
    });

    world.send(AddPlayers {
        fd: add_player,
        servers,
    });
    world.send(RemovePlayers {
        fd: remove_player,
        servers,
    });
    world.send(SentData { decrease_count });
    world.send(RecvAllData { inner: recv_data });
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn add_player(
    r: ReceiverMut<AddPlayers>,
    mut fd_lookup: Single<&mut FdLookup>,
    mut buffer_allocator: Single<&mut BufferAllocators>,
    mut sender: IngressSender,
) {
    let mut event = r.event;

    let event = &mut *event;

    let fds = &mut event.fd;
    let servers = &mut event.servers;

    for (idx, elems) in fds.iter().enumerate() {
        let server = event.servers.get_mut(idx).unwrap();
        let allocator = buffer_allocator.get_mut(idx).unwrap();

        for &fd in elems {
            let new_player = sender.spawn();

            sender.insert(new_player, LoginState::Handshake);
            sender.insert(new_player, DecodeBuffer::default());
            sender.insert(new_player, Packets::new(allocator).unwrap());

            sender.insert(new_player, fd);

            server.fd_ids.insert(new_player);

            fd_lookup.insert(fd, new_player);
            // trace!("got a player with fd {:?}", fd);
        }
    }
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn remove_player(
    r: ReceiverMut<RemovePlayers>,
    mut fd_lookup: Single<&mut FdLookup>,
    mut sender: IngressSender,
) {
    for &fd in r.event.fd.iter().flatten() {}

    let mut event = r.event;

    let event = &mut *event;

    let fds = &mut event.fd;

    for (idx, elems) in fds.iter().enumerate() {
        let server = event.servers.get_mut(idx).unwrap();

        for &fd in elems {
            let Some(id) = fd_lookup.remove(&fd) else {
                warn!("tried to remove player with fd {fd:?} but it seemed to already be removed",);
                return;
            };

            debug_assert!(server.fd_ids.remove(&id));

            sender.despawn(id);

            // trace!("removed a player with id {id:?}");
        }
    }
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn sent_data(r: Receiver<SentData>, players: Fetcher<&Packets>, fd_lookup: Single<&FdLookup>) {
    let event = r.event;

    // players.get

    // todo: par iter
    event
        .decrease_count
        .get_all()
        .par_iter()
        .flatten()
        .for_each(|(fd, count)| {
            let Some(&id) = fd_lookup.get(fd) else {
                warn!(
                    "tried to get id for fd {:?} but it seemed to already be removed",
                    fd
                );
                return;
            };

            let Ok(pkts) = players.get(id) else {
                warn!(
                    "tried to get pkts for id {:?} but it seemed to already be removed",
                    id
                );
                return;
            };

            pkts.set_successfully_sent(*count);
        });
}

#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn recv_data(
    r: ReceiverMut<RecvAllData>,
    mut fd_lookup: Single<&mut FdLookup>,
    mut sender: IngressSender,
    global: Single<&Global>,
    mut players: Fetcher<(
        &mut LoginState,
        &mut DecodeBuffer,
        &mut Packets,
        &Fd,
        Option<&mut FullEntityPose>,
        Option<&mut Vitals>,
        Option<&mut KeepAlive>,
        Option<&mut ImmuneStatus>,
    )>,
    id_lookup: Single<&EntityIdLookup>,
    compose: Compose,
) {
    let event = r.event;

    for (fd, data) in event.inner.iter().flatten().copied() {
        // trace!("got data: {data:?}");
        let Some(&id) = fd_lookup.get(&fd) else {
            warn!("got data for fd that is not in the fd lookup: {fd:?}");
            return;
        };

        let (login_state, decoder, packets, _, mut pose, mut vitals, mut keep_alive, mut immunity) =
            players.get_mut(id).expect("player with fd not found");

        decoder.queue_slice(data);

        // todo: error  on low compression: "decompressed packet length of 2 is <= the compression threshold of 2"
        let scratch = compose.scratch.get_local();
        let mut scratch = scratch.borrow_mut();
        let scratch = &mut *scratch;

        while let Some(frame) = decoder.try_next_packet(scratch).unwrap() {
            match *login_state {
                LoginState::Handshake => process_handshake(login_state, &frame).unwrap(),
                LoginState::Status => {
                    process_status(login_state, &frame, packets).unwrap();
                }
                LoginState::Terminate => {
                    // todo: does this properly terminate the connection? I don't think so probably
                    let Some(id) = fd_lookup.remove(&fd) else {
                        return;
                    };

                    sender.despawn(id);
                }
                LoginState::Login => {
                    process_login(
                        id,
                        login_state,
                        &frame,
                        packets,
                        decoder,
                        &global,
                        &mut sender,
                    )
                    .unwrap();
                }
                LoginState::TransitioningPlay { .. } | LoginState::Play => {
                    if let LoginState::TransitioningPlay {
                        packets_to_transition,
                    } = login_state
                    {
                        if *packets_to_transition == 0 {
                            *login_state = LoginState::Play;
                        } else {
                            *packets_to_transition -= 1;
                        }
                    }

                    if let Some((pose, vitals, keep_alive, immunity)) =
                        itertools::izip!(&mut pose, &mut vitals, &mut keep_alive, &mut immunity)
                            .next()
                    {
                        let mut query = PacketSwitchQuery {
                            id,
                            pose,
                            vitals,
                            keep_alive,
                            immunity,
                        };

                        crate::packets::switch(frame, &global, &mut sender, &id_lookup, &mut query)
                            .unwrap();
                    }
                }
            }
        }
    }

    // this is important so broadcast order is not before player gets change to play
}

fn process_handshake(login_state: &mut LoginState, packet: &PacketFrame) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Handshake);

    let handshake: packets::handshaking::HandshakeC2s = packet.decode()?;

    trace!("received handshake: {:?}", handshake);

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
    id: EntityId,
    login_state: &mut LoginState,
    packet: &PacketFrame,
    packets: &mut Packets,
    decoder: &mut DecodeBuffer,
    global: &Global,
    sender: &mut IngressSender,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Login);

    let login::LoginHelloC2s { username, .. } = packet.decode()?;

    trace!("received LoginHello for {username}");

    let username = username.0;

    let pkt = LoginCompressionS2c {
        threshold: VarInt(global.shared.compression_threshold.0),
    };

    packets.append_pre_compression_packet(&pkt)?;

    decoder.set_compression(global.shared.compression_threshold);

    let username = Box::from(username);

    sender.send(event::PlayerInit {
        target: id,
        username,
        pose: FullEntityPose::player(),
    });

    // todo: impl rest
    *login_state = LoginState::TransitioningPlay {
        packets_to_transition: 5,
    };

    Ok(())
}

fn process_status(
    login_state: &mut LoginState,
    packet: &PacketFrame,
    packets: &mut Packets,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Status);

    #[allow(clippy::single_match, reason = "todo del")]
    match packet.id {
        packets::status::QueryRequestC2s::ID => {
            let query_request: packets::status::QueryRequestC2s = packet.decode()?;

            // https://wiki.vg/Server_List_Ping#Response
            let json = json!({
                "version": {
                    "name": MINECRAFT_VERSION,
                    "protocol": PROTOCOL_VERSION,
                },
                "players": {
                    "online": 1,
                    "max": 32,
                    "sample": [],
                },
                "description": "something"
            });

            let json = serde_json::to_string_pretty(&json)?;

            let send = packets::status::QueryResponseS2c { json: &json };

            info!("sent query response: {query_request:?}");
            //
            packets.append_pre_compression_packet(&send)?;

            // send pong
            // we send this right away so our ping looks better
            // let send = packets::status::QueryPongS2c { payload: 123 };
            // packets.append_pre_compression_packet(&send, io, scratch)?;

            // short circuit
            // *login_state = LoginState::Terminate;
        }

        packets::status::QueryPingC2s::ID => {
            let query_ping: packets::status::QueryPingC2s = packet.decode()?;

            let payload = query_ping.payload;

            let send = packets::status::QueryPongS2c { payload };

            packets.append_pre_compression_packet(&send)?;

            info!("sent query pong: {query_ping:?}");
            *login_state = LoginState::Terminate;
        }

        _ => panic!("unexpected packet id: {}", packet.id),
    }

    // todo: check version is correct

    Ok(())
}
