use std::borrow::Cow;

use anyhow::Context;
use evenio::{
    event::{Despawn, Event, Insert, Receiver, Sender, Spawn},
    fetch::{Fetcher, Single},
    prelude::{EntityId, ReceiverMut},
    world::World,
};
use serde_json::json;
use sha2::Digest;
use tracing::{info, instrument, trace, warn};
use uuid::Uuid;
use valence_protocol::{
    decode::PacketFrame,
    packets,
    packets::{handshaking::handshake_c2s::HandshakeNextState, login, login::LoginCompressionS2c},
    Bounded, Packet, VarInt,
};

use crate::{
    global::Global,
    net::{Server, ServerDef, ServerEvent},
    singleton::fd_lookup::FdLookup,
};

mod player_packet_buffer;

use crate::{
    components::{FullEntityPose, ImmuneStatus, KeepAlive, LoginState, Vitals},
    events::{
        AttackEntity, BumpScratch, InitEntity, KickPlayer, KillAllEntities, PlayerInit,
        ScratchBuffer, SwingArm,
    },
    net::{Fd, IoBuf, Packets, MINECRAFT_VERSION, PROTOCOL_VERSION},
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
        Insert<IoBuf>,
        Insert<Fd>,
        Insert<Packets>,
        PlayerInit,
        Despawn,
        KickPlayer,
        InitEntity,
        KillAllEntities,
        SwingArm,
        AttackEntity,
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
    scratch: &'b mut BumpScratch<'a>,
}

#[derive(Event)]
pub struct SentData {
    fd: Fd,
}

// todo: remove
#[expect(
    clippy::non_send_fields_in_send_ty,
    reason = "todo we will remove this"
)]
unsafe impl<'a, 'b, 'c> Send for RecvData<'a, 'b, 'c> {}
unsafe impl<'a, 'b, 'c> Sync for RecvData<'a, 'b, 'c> {}

pub fn generate_ingress_events(world: &mut World, server: &mut Server, scratch: &mut BumpScratch) {
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
                world.send(SentData { fd });
            }
        })
        .unwrap();
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
    info!("got a player with fd {:?}", fd);
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
        warn!(
            "tried to remove player with fd {:?} but it seemed to already be removed",
            fd
        );
        return;
    };

    sender.despawn(id);

    info!("removed a player with fd {:?}", fd);
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn sent_data(
    r: Receiver<SentData>,
    mut players: Fetcher<&mut Packets>,
    fd_lookup: Single<&FdLookup>,
) {
    let event = r.event;

    let fd = event.fd;
    let Some(id) = fd_lookup.get(&fd) else {
        warn!(
            "tried to get id for fd {:?} but it seemed to already be removed",
            fd
        );
        return;
    };

    let Ok(pkts) = players.get_mut(*id) else {
        warn!(
            "tried to get pkts for id {:?} but it seemed to already be removed",
            id
        );
        return;
    };

    pkts.set_successfully_sent();
}

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
    mut io: Single<&mut IoBuf>,
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

    // todo: error  on low compression: "decompressed packet length of 2 is <= the compression threshold of 2"
    while let Some(frame) = decoder.try_next_packet().unwrap() {
        match *login_state {
            LoginState::Handshake => process_handshake(login_state, &frame).unwrap(),
            LoginState::Status => {
                process_status(login_state, &frame, packets, scratch, &mut io).unwrap();
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
                    &mut io,
                    scratch,
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

/// Get a [`Uuid`] based on the given user's name.
fn offline_uuid(username: &str) -> anyhow::Result<Uuid> {
    let digest = sha2::Sha256::digest(username);

    #[expect(clippy::indexing_slicing, reason = "sha256 is always 32 bytes")]
    let slice = &digest[..16];

    Uuid::from_slice(slice).context("failed to create uuid")
}

#[allow(clippy::too_many_arguments, reason = "todo del")]
fn process_login(
    id: EntityId,
    login_state: &mut LoginState,
    packet: &PacketFrame,
    packets: &mut Packets,
    decoder: &mut DecodeBuffer,
    global: &Global,
    io: &mut IoBuf,
    scratch: &mut impl ScratchBuffer,
    sender: &mut IngressSender,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Login);

    let login::LoginHelloC2s { username, .. } = packet.decode()?;

    info!("received LoginHello for {username}");

    let username = username.0;
    let uuid = offline_uuid(username)?;

    let pkt = LoginCompressionS2c {
        threshold: VarInt(global.shared.compression_level.0),
    };

    packets.append_pre_compression_packet(&pkt, io, scratch)?;

    decoder.set_compression(global.shared.compression_level);

    let pkt = login::LoginSuccessS2c {
        uuid,
        username: Bounded(username),

        // todo: do any properties make sense to include?
        properties: Cow::default(),
    };

    packets.append(&pkt, io, scratch)?;

    let username = Box::from(username);

    // todo: impl rest
    *login_state = LoginState::TransitioningPlay {
        packets_to_transition: 5,
    };

    sender.send(PlayerInit {
        entity: id,
        username,
        uuid,
        pose: FullEntityPose::player(),
    });

    Ok(())
}

fn process_status(
    login_state: &mut LoginState,
    packet: &PacketFrame,
    packets: &mut Packets,
    scratch: &mut impl ScratchBuffer,
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
            packets.append_pre_compression_packet(&send, io, scratch)?;
            // we send this right away so our ping looks better
            // let send = packets::status::QueryPongS2c { payload: 123 };
            // packets.append_pre_compression_packet(&send, io)?;

            // short circuit
            // *login_state = LoginState::Terminate;
        }

        packets::status::QueryPingC2s::ID => {
            let query_ping: packets::status::QueryPingC2s = packet.decode()?;

            let payload = query_ping.payload;

            let send = packets::status::QueryPongS2c { payload };

            packets.append_pre_compression_packet(&send, io, scratch)?;

            info!("sent query response: {query_ping:?}");
            *login_state = LoginState::Terminate;
        }

        _ => panic!("unexpected packet id: {}", packet.id),
    }

    // todo: check version is correct

    Ok(())
}
