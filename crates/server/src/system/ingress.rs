use arrayvec::ArrayVec;
use derive_more::From;
use evenio::{
    event::{Despawn, Event, EventMut, Insert, Receiver, Sender, Spawn},
    fetch::{Fetcher, Single},
    prelude::{EntityId, ReceiverMut},
    world::World,
};
use fxhash::FxHashMap;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use rayon_local::RayonLocal;
use serde_json::json;
use tracing::{info, instrument, span, trace, warn, Level};
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
    CowBytes,
};

mod player_packet_buffer;

use crate::{
    components::{FullEntityPose, ImmuneStatus, KeepAlive, LoginState, LoginStatePendingC2s, LoginStatePendingS2c, Vitals},
    net::{Compose, Fd, MINECRAFT_VERSION, PROTOCOL_VERSION},
    packets::PacketSwitchQuery,
    singleton::player_id_lookup::EntityIdLookup,
    system::ingress::player_packet_buffer::DecodeBuffer,
};

pub type IngressSender<'a> = Sender<
    'a,
    (
        event::PlayerInit,
        event::KickPlayer,
        event::InitEntity,
        event::SwingArm,
        event::AttackEntity,
        event::BlockStartBreak,
        event::BlockAbortBreak,
        event::BlockFinishBreak,
        event::Command,
        event::PoseUpdate,
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
pub struct RecvDataBulk<'a> {
    elements: FxHashMap<Fd, ArrayVec<CowBytes<'a>, 16>>,
}

#[instrument(skip_all, level = "trace")]
pub fn generate_ingress_events(world: &mut World, server: &mut Server) {
    // `let mut decrease_count = FxHashMap::default();

    let mut recv_data_elements: FxHashMap<Fd, ArrayVec<CowBytes, 16>> = FxHashMap::default();

    server
        .drain(|event| match event {
            ServerEvent::AddPlayer { fd } => {
                world.send(AddPlayer { fd });
            }
            ServerEvent::RemovePlayer { fd } => {
                world.send(RemovePlayer { fd });
            }
            ServerEvent::RecvData { fd, data } => {
                recv_data_elements.entry(fd).or_default().push(data);
            }
            ServerEvent::SentData { fd } => {
                // TODO: check for truncated writes
            }
        })
        .unwrap();

    span!(Level::TRACE, "sent-data").in_scope(|| {
        // world.send(SentData { decrease_count });
    });

    span!(Level::TRACE, "recv-data").in_scope(|| {
        world.send(RecvDataBulk {
            elements: recv_data_elements,
        });
    });
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn add_player(
    r: ReceiverMut<AddPlayer>,
    mut fd_lookup: Single<&mut FdLookup>,
    mut sender: Sender<(
        Spawn,
        Insert<LoginState>,
        Insert<DecodeBuffer>,
        Insert<Fd>,
    )>,
) {
    let event = r.event;

    let new_player = sender.spawn();
    sender.insert(new_player, LoginState::PendingC2s(LoginStatePendingC2s::Handshake));
    sender.insert(new_player, DecodeBuffer::default());

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
    mut sender: Sender<Despawn>,
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

#[derive(From)]
pub enum SendElem {
    PlayerInit(event::PlayerInit),
    KickPlayer(event::KickPlayer),
    InitEntity(event::InitEntity),
    SwingArm(event::SwingArm),
    AttackEntity(event::AttackEntity),
    BlockStartBreak(event::BlockStartBreak),
    BlockAbortBreak(event::BlockAbortBreak),
    BlockFinishBreak(event::BlockFinishBreak),
    Command(event::Command),
    PoseUpdate(event::PoseUpdate),
}

#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn recv_data(
    r: ReceiverMut<RecvDataBulk>,
    fd_lookup: Single<&mut FdLookup>,
    global: Single<&Global>,
    mut players: Fetcher<(
        &mut LoginState,
        &mut DecodeBuffer,
        &Fd,
        Option<&mut FullEntityPose>,
        Option<&mut Vitals>,
        Option<&mut KeepAlive>,
        Option<&mut ImmuneStatus>,
    )>,
    id_lookup: Single<&EntityIdLookup>,
    mut real_sender: IngressSender,
    compose: Compose,
) {
    let event = EventMut::take(r.event);

    let send_events = RayonLocal::init(Vec::new);
    let elements = event.elements;

    players.par_iter_mut().for_each(
        |(
            login_state,
            decoder,
            fd,
            mut pose,
            mut vitals,
            mut keep_alive,
            mut immunity,
        )| {
            let Some(data) = elements.get(fd) else {
                return;
            };

            for data in data {
                trace!("got data: {data:?}");
                let Some(&id) = fd_lookup.get(fd) else {
                    warn!("got data for fd that is not in the fd lookup: {fd:?}");
                    return;
                };

                decoder.queue_slice(data.as_ref());

                let scratch = compose.scratch.get_local();
                let mut scratch = scratch.borrow_mut();
                let scratch = &mut *scratch;

                let sender = send_events.get_local_raw();
                let sender = unsafe { &mut *sender.get() };

                // todo: error  on low compression: "decompressed packet length of 2 is <= the compression threshold of 2"
                while let Some(frame) = decoder.try_next_packet(scratch).unwrap() {
                    tracing::info!("ingress: login state: {login_state:?}");
                    match login_state {
                        LoginState::PendingC2s(state) => match state {
                            LoginStatePendingC2s::Handshake => process_handshake(login_state, &frame).unwrap(),
                            LoginStatePendingC2s::StatusRequest => process_status_request(login_state, &frame).unwrap(),
                            LoginStatePendingC2s::StatusPing => process_status_ping(login_state, &frame).unwrap(),
                        },
                        LoginState::PendingS2c(_) => {},
//                        LoginState::Login => {
//                            process_login(
//                                id,
//                                login_state,
//                                &frame,
//                                decoder,
//                                &global,
//                                sender,
//                            )
//                            .unwrap();
//                        }
                        LoginState::Play => {
                            if let Some((pose, vitals, keep_alive, immunity)) = itertools::izip!(
                                pose.as_mut(),
                                vitals.as_mut(),
                                keep_alive.as_mut(),
                                immunity.as_mut()
                            )
                            .next()
                            {
                                let mut query = PacketSwitchQuery {
                                    id,
                                    pose,
                                    vitals,
                                    keep_alive,
                                    immunity,
                                };

                                crate::packets::switch(
                                    frame, &global, sender, &id_lookup, &mut query,
                                )
                                .unwrap();
                            }
                        }
                    }
                    tracing::info!("ingress: new login state: {login_state:?}");
                }
            }
        },
    );

    for elem in send_events.into_iter().flatten() {
        match elem {
            SendElem::PlayerInit(event) => {
                real_sender.send(event);
            }
            SendElem::KickPlayer(event) => {
                real_sender.send(event);
            }
            SendElem::InitEntity(event) => {
                real_sender.send(event);
            }
            SendElem::SwingArm(event) => {
                real_sender.send(event);
            }
            SendElem::AttackEntity(event) => {
                real_sender.send(event);
            }
            SendElem::BlockStartBreak(event) => {
                real_sender.send(event);
            }
            SendElem::BlockAbortBreak(event) => {
                real_sender.send(event);
            }
            SendElem::BlockFinishBreak(event) => {
                real_sender.send(event);
            }
            SendElem::Command(event) => {
                real_sender.send(event);
            }
            SendElem::PoseUpdate(event) => {
                real_sender.send(event);
            }
        }
    }

    // this is important so broadcast order is not before player gets change to play
}

fn process_handshake(login_state: &mut LoginState, packet: &PacketFrame) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::PendingC2s(LoginStatePendingC2s::Handshake));

    let handshake: packets::handshaking::HandshakeC2s = packet.decode()?;

    trace!("received handshake: {:?}", handshake);

    // todo: check version is correct

    match handshake.next_state {
        HandshakeNextState::Status => {
            *login_state = LoginState::PendingC2s(LoginStatePendingC2s::StatusRequest);
        }
        HandshakeNextState::Login => {
            // TODO:
            // *login_state = LoginState::Login;
        }
    }

    Ok(())
}

//#[allow(clippy::too_many_arguments, reason = "todo del")]
//fn process_login(
//    id: EntityId,
//    login_state: &mut LoginState,
//    packet: &PacketFrame,
//    decoder: &mut DecodeBuffer,
//    global: &Global,
//    sender: &mut Vec<SendElem>,
//) -> anyhow::Result<()> {
//    debug_assert!(*login_state == LoginState::Login);
//
//    let login::LoginHelloC2s { username, .. } = packet.decode()?;
//
//    trace!("received LoginHello for {username}");
//
//    let username = username.0;
//
//    let pkt = LoginCompressionS2c {
//        threshold: VarInt(global.shared.compression_threshold.0),
//    };
//
//    //packets.append_pre_compression_packet(&pkt)?;
//
//    decoder.set_compression(global.shared.compression_threshold);
//
//    let username = Box::from(username);
//
//    sender.push(
//        event::PlayerInit {
//            target: id,
//            username,
//            pose: FullEntityPose::player(),
//        }
//        .into(),
//    );
//
//    // todo: impl rest
//    *login_state = LoginState::TransitioningPlay {
//        packets_to_transition: 5,
//    };
//
//    Ok(())
//}

fn process_status_request(
    login_state: &mut LoginState,
    packet: &PacketFrame,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::PendingC2s(LoginStatePendingC2s::StatusRequest));

    if packet.id != packets::status::QueryRequestC2s::ID {
        anyhow::bail!("unexpected c2s packet; expected status request");
    }

    *login_state = LoginState::PendingS2c(LoginStatePendingS2c::StatusResponse);

    Ok(())
}

fn process_status_ping(
    login_state: &mut LoginState,
    packet: &PacketFrame,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::PendingC2s(LoginStatePendingC2s::StatusPing));

    let query_ping: packets::status::QueryPingC2s = packet.decode()?;
    let payload = query_ping.payload;

    *login_state = LoginState::PendingS2c(LoginStatePendingS2c::StatusPong { payload });

    Ok(())
}
