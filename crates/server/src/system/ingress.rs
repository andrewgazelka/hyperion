use evenio::{
    event::{Despawn, Event, Insert, Receiver, Sender, Spawn},
    fetch::{Fetcher, Single},
    prelude::{EntityId, ReceiverMut},
    world::World,
};
use fxhash::FxHashMap;
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
    event::{self, UpdateSelectedSlot},
    global::Global,
    net::{Server, ServerDef, ServerEvent},
    singleton::fd_lookup::FdLookup,
};

mod player_packet_buffer;

use crate::{
    components::{FullEntityPose, ImmuneStatus, KeepAlive, LoginState, Vitals},
    event::BumpScratch,
    net::{Fd, IoBuf, IoBufs, Packets, MINECRAFT_VERSION, PROTOCOL_VERSION},
    packets::PacketSwitchQuery,
    singleton::player_id_lookup::EntityIdLookup,
    system::ingress::player_packet_buffer::DecodeBuffer,
};

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
        (event::Command, event::PoseUpdate, UpdateSelectedSlot),
    ),
>;

#[derive(Event)]
pub struct AddPlayer {
    fd: Fd,
}

#[derive(Event)]
pub struct RemovePlayer {
    fd: Fd,
}

// todo: do we really need three different lifetimes here?
#[derive(Event)]
pub struct RecvData<'a, 'b, 'c> {
    fd: Fd,
    data: &'c [u8],
    scratch: &'b mut RayonLocal<BumpScratch<'a>>,
}

#[derive(Event)]
pub struct SentData {
    decrease_count: FxHashMap<Fd, usize>,
}

#[instrument(skip_all, level = "trace")]
pub fn generate_ingress_events(
    world: &mut World,
    server: &mut Server,
    scratch: &mut RayonLocal<BumpScratch>,
) {
    let mut decrease_count = FxHashMap::default();

    server
        .drain(|event| match event {
            ServerEvent::AddPlayer { fd } => {
                world.send(AddPlayer { fd });
            }
            ServerEvent::RemovePlayer { fd } => {
                world.send(RemovePlayer { fd });
            }
            ServerEvent::RecvData { fd, data } => {
                world.send(RecvData { fd, data, scratch });
            }
            ServerEvent::SentData { fd } => {
                decrease_count
                    .entry(fd)
                    .and_modify(|x| *x += 1)
                    .or_insert(1);
            }
        })
        .unwrap();

    world.send(SentData { decrease_count });
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn add_player(
    r: ReceiverMut<AddPlayer>,
    mut fd_lookup: Single<&mut FdLookup>,
    mut sender: IngressSender,
) {
    let event = r.event;

    let new_player = sender.spawn();
    sender.insert(new_player, LoginState::Handshake);
    sender.insert(new_player, DecodeBuffer::default());

    sender.insert(new_player, Packets::default());
    let fd = event.fd;
    sender.insert(new_player, fd);

    fd_lookup.insert(fd, new_player);
    trace!("got a player with fd {:?}", fd);
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn remove_player(
    r: ReceiverMut<RemovePlayer>,
    mut fd_lookup: Single<&mut FdLookup>,
    mut sender: IngressSender,
) {
    let event = r.event;

    let fd = event.fd;
    let Some(id) = fd_lookup.remove(&fd) else {
        warn!("tried to remove player with fd {fd:?} but it seemed to already be removed",);
        return;
    };

    sender.despawn(id);

    trace!("removed a player with fd {:?}", fd);
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn sent_data(r: Receiver<SentData>, players: Fetcher<&Packets>, fd_lookup: Single<&FdLookup>) {
    let event = r.event;

    // todo: par iter
    event.decrease_count.iter().for_each(|(fd, count)| {
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
    r: ReceiverMut<RecvData>,
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
    mut io: Single<&mut IoBufs>,
) {
    let mut event = r.event;

    let fd = event.fd;
    let data = event.data;
    // todo: again why do we need &mut * ... also seems to borrow the entire event sadly
    let scratch = &mut *event.scratch;

    trace!("got data: {data:?}");
    let Some(&id) = fd_lookup.get(&fd) else {
        warn!("got data for fd that is not in the fd lookup: {fd:?}");
        return;
    };

    let (login_state, decoder, packets, _, mut pose, mut vitals, mut keep_alive, mut immunity) =
        players.get_mut(id).expect("player with fd not found");

    decoder.queue_slice(data);

    let scratch = scratch.one();

    let io = io.one();

    // todo: error  on low compression: "decompressed packet length of 2 is <= the compression threshold of 2"
    while let Some(frame) = decoder.try_next_packet(scratch).unwrap() {
        match *login_state {
            LoginState::Handshake => process_handshake(login_state, &frame).unwrap(),
            LoginState::Status => {
                let io = io.get_mut();
                process_status(login_state, &frame, packets, io).unwrap();
            }
            LoginState::Terminate => {
                // todo: does this properly terminate the connection? I don't think so probably
                let Some(id) = fd_lookup.remove(&fd) else {
                    return;
                };

                sender.despawn(id);
            }
            LoginState::Login => {
                let io = io.get_mut();
                process_login(
                    id,
                    login_state,
                    &frame,
                    packets,
                    decoder,
                    &global,
                    io,
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
                    itertools::izip!(&mut pose, &mut vitals, &mut keep_alive, &mut immunity).next()
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
    packets: &Packets,
    decoder: &mut DecodeBuffer,
    global: &Global,
    io: &mut IoBuf,
    sender: &mut IngressSender,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Login);

    let login::LoginHelloC2s { username, .. } = packet.decode()?;

    trace!("received LoginHello for {username}");

    let username = username.0;

    let pkt = LoginCompressionS2c {
        threshold: VarInt(global.shared.compression_threshold.0),
    };

    packets.append_pre_compression_packet(&pkt, io)?;

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
    packets: &Packets,
    io: &mut IoBuf,
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
            packets.append_pre_compression_packet(&send, io)?;

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

            packets.append_pre_compression_packet(&send, io)?;

            info!("sent query pong: {query_ping:?}");
            *login_state = LoginState::Terminate;
        }

        _ => panic!("unexpected packet id: {}", packet.id),
    }

    // todo: check version is correct

    Ok(())
}
