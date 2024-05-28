use derive_more::From;
use evenio::{
    entity::EntityId,
    event::{Despawn, EventMut, GlobalEvent, Insert, Receiver, ReceiverMut, Sender, Spawn},
    fetch::{Fetcher, Single},
    world::World,
};
use fxhash::FxHashMap;
use parking_lot::Mutex;
use rayon::iter::ParallelIterator;
use serde_json::json;
use tracing::{instrument, trace, warn};
use valence_protocol::{
    decode::PacketFrame,
    packets,
    packets::{handshaking::handshake_c2s::HandshakeNextState, login, login::LoginCompressionS2c},
    Packet, VarInt,
};

use crate::{
    event::{self},
    global::Global,
    singleton::fd_lookup::StreamLookup,
};

mod player_packet_buffer;

use crate::{
    components::{FullEntityPose, LoginState},
    net::{Compose, Packets, MINECRAFT_VERSION, PROTOCOL_VERSION},
    singleton::{fd_lookup::StreamId, player_id_lookup::EntityIdLookup},
    system::ingress::player_packet_buffer::DecodeBuffer,
};
use crate::net::proxy::ReceiveStateInner;

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
        event::UpdateSelectedSlot,
        event::ClickEvent,
    ),
>;

#[derive(GlobalEvent)]
pub struct AddPlayer {
    stream: u64,
}

#[derive(GlobalEvent)]
pub struct RemovePlayer {
    stream: u64,
}

// todo: do we really need three different lifetimes here?
#[derive(GlobalEvent)]
pub struct RecvDataBulk<'a> {
    elements: &'a mut targeted_bulk::TargetedEvents<bytes::Bytes, u64>,
}

#[derive(GlobalEvent)]
pub struct SentData {
    decrease_count: FxHashMap<StreamId, u8>,
}

#[instrument(skip_all, level = "trace")]
#[allow(
    clippy::significant_drop_in_scrutinee,
    reason = "I think this is fine. However, let's double check in perf"
)]
pub fn generate_ingress_events(world: &mut World, shared: &Mutex<ReceiveStateInner>) {
    // todo: should we just lock once?

    let mut recv = shared.lock();

    for connect in recv.player_connect.drain(..) {
        world.send(AddPlayer {
            stream: connect.stream,
        });
    }

    for disconnect in recv.player_disconnect.drain(..) {
        world.send(RemovePlayer {
            stream: disconnect.stream,
        });
    }

    world.send(RecvDataBulk {
        elements: &mut recv.packets,
    });

    recv.packets.clear();
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn add_player(
    r: ReceiverMut<AddPlayer>,
    fd_lookup: Single<&mut StreamLookup>,
    sender: Sender<(
        Spawn,
        Insert<LoginState>,
        Insert<DecodeBuffer>,
        Insert<Packets>,
    )>,
) {
    let event = r.event;

    let new_player = sender.spawn();
    sender.insert(new_player, LoginState::Handshake);
    sender.insert(new_player, DecodeBuffer::default());

    trace!("got a player with stream {}", event.stream);

    sender.insert(new_player, Packets::new(event.stream));
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn remove_player(
    r: ReceiverMut<RemovePlayer>,
    mut fd_lookup: Single<&mut StreamLookup>,
    sender: Sender<Despawn>,
) {
    let event = r.event;

    let stream = event.stream;

    let Some(id) = fd_lookup.remove(&stream) else {
        warn!("tried to remove player with stream {stream:?} but it seemed to already be removed",);
        return;
    };

    sender.despawn(id);

    trace!("removed a player with stream {stream}");
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn sent_data(
    r: Receiver<SentData>,
    mut players: Fetcher<&mut Packets>,
    fd_lookup: Single<&StreamLookup>,
) {
    let event = r.event;

    // todo: par iter
    event.decrease_count.iter().for_each(|(fd, count)| {
        let Some(&id) = fd_lookup.get(fd) else {
            trace!(
                "tried to get id for fd {:?} but it seemed to already be removed",
                fd
            );
            return;
        };

        let Ok(pkts) = players.get_mut(id) else {
            warn!(
                "tried to get pkts for id {:?} but it seemed to already be removed",
                id
            );
            return;
        };
    });
}

#[derive(From)]
pub enum SendData {
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
    UpdateSelectedSlot(event::UpdateSelectedSlot),
    ClickEvent(event::ClickEvent),
}

pub struct SendElem {
    id: EntityId,
    data: SendData,
}

impl SendElem {
    pub fn new(id: EntityId, data: impl Into<SendData>) -> Self {
        Self {
            id,
            data: data.into(),
        }
    }
}

#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn recv_data(
    r: ReceiverMut<RecvDataBulk>,
    fd_lookup: Single<&mut StreamLookup>,
    global: Single<&Global>,
    players: Fetcher<(
        &mut LoginState,
        &mut DecodeBuffer,
        &Packets,
        Option<&mut FullEntityPose>,
    )>,
    id_lookup: Single<&EntityIdLookup>,
    real_sender: IngressSender,
    compose: Compose,
) {
    let event = EventMut::take(r.event);

    // let send_events = RayonLocal::init(Vec::new);
    // let elements = event.elements;

    // players
    //     .par_iter_mut()
    //     .for_each(|(login_state, decoder, packets, mut pose)| {
    //         let Some(data) = elements.get(fd) else {
    //             return;
    //         };
    //
    //         for data in data {
    //             trace!("got data: {data:?}");
    //             let Some(&id) = fd_lookup.get(fd) else {
    //                 warn!("got data for fd that is not in the fd lookup: {fd:?}");
    //                 return;
    //             };
    //
    //             decoder.queue_slice(data.as_ref());
    //
    //             let scratch = compose.scratch();
    //             let mut scratch = scratch.borrow_mut();
    //             let scratch = &mut *scratch;
    //
    //             let sender = send_events.get_local_raw();
    //             let sender = unsafe { &mut *sender.get() };
    //
    //             // todo: error  on low compression: "decompressed packet length of 2 is <= the compression threshold of 2"
    //             while let Some(frame) = decoder.try_next_packet(scratch).unwrap() {
    //                 match *login_state {
    //                     LoginState::Handshake => process_handshake(login_state, &frame).unwrap(),
    //                     LoginState::Status => {
    //                         process_status(login_state, &frame, packets).unwrap();
    //                     }
    //                     LoginState::Terminate => {
    //                         // // todo: does this properly terminate the connection? I don't think so probably
    //                         // let Some(id) = fd_lookup.remove(&fd) else {
    //                         //     return;
    //                         // };
    //                         //
    //                         // sender.despawn(id);
    //                     }
    //                     LoginState::Login => {
    //                         process_login(
    //                             id,
    //                             login_state,
    //                             &frame,
    //                             packets,
    //                             decoder,
    //                             &global,
    //                             sender,
    //                         )
    //                         .unwrap();
    //                     }
    //                     LoginState::TransitioningPlay { .. } | LoginState::Play => {
    //                         if let LoginState::TransitioningPlay {
    //                             packets_to_transition,
    //                         } = login_state
    //                         {
    //                             if *packets_to_transition == 0 {
    //                                 *login_state = LoginState::Play;
    //                             } else {
    //                                 *packets_to_transition -= 1;
    //                             }
    //                         }
    //
    //                         if let Some(pose) = pose.as_mut() {
    //                             let mut query = PacketSwitchQuery { id, pose };
    //
    //                             crate::packets::switch(&frame, sender, &id_lookup, &mut query)
    //                                 .unwrap();
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     });
    //
    // for SendElem { id, data } in send_events.into_iter().flatten() {
    //     match data {
    //         SendData::PlayerInit(event) => real_sender.send_to(id, event),
    //         SendData::KickPlayer(event) => real_sender.send_to(id, event),
    //         SendData::InitEntity(event) => real_sender.send(event),
    //         SendData::SwingArm(event) => real_sender.send_to(id, event),
    //         SendData::AttackEntity(event) => real_sender.send_to(id, event),
    //         SendData::BlockStartBreak(event) => real_sender.send_to(id, event),
    //         SendData::BlockAbortBreak(event) => real_sender.send_to(id, event),
    //         SendData::BlockFinishBreak(event) => real_sender.send_to(id, event),
    //         SendData::Command(event) => real_sender.send_to(id, event),
    //         SendData::PoseUpdate(event) => real_sender.send_to(id, event),
    //         SendData::UpdateSelectedSlot(event) => real_sender.send_to(id, event),
    //         SendData::ClickEvent(event) => real_sender.send_to(id, event),
    //     }
    // }

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
    sender: &mut Vec<SendElem>,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Login);

    let login::LoginHelloC2s { username, .. } = packet.decode()?;

    trace!("received LoginHello for {username}");

    let username = username.0;

    let pkt = LoginCompressionS2c {
        threshold: VarInt(global.shared.compression_threshold.0),
    };

    // packets.append_pre_compression_packet(&pkt)?;

    decoder.set_compression(global.shared.compression_threshold);

    let username = Box::from(username);

    let elem = SendElem::new(id, event::PlayerInit {
        username,
        pose: FullEntityPose::player(),
    });

    sender.push(elem);

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

            trace!("sent query response: {query_request:?}");
            // packets.append_pre_compression_packet(&send)?;
        }

        packets::status::QueryPingC2s::ID => {
            let query_ping: packets::status::QueryPingC2s = packet.decode()?;

            let payload = query_ping.payload;

            let send = packets::status::QueryPongS2c { payload };

            // packets.append_pre_compression_packet(&send)?;

            trace!("sent query pong: {query_ping:?}");
            *login_state = LoginState::Terminate;
        }

        _ => panic!("unexpected packet id: {}", packet.id),
    }

    // todo: check version is correct

    Ok(())
}
