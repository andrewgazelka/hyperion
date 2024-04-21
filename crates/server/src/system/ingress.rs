use std::borrow::Cow;

use anyhow::Context;
use evenio::{
    event::{Despawn, Insert, Sender, Spawn},
    fetch::{Fetcher, Single},
    prelude::{EntityId, ReceiverMut},
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
    components::{FullEntityPose, LoginState},
    events::{
        AttackEntity, Gametick, InitEntity, KickPlayer, KillAllEntities, PlayerInit, ScratchBuffer,
        SwingArm,
    },
    net::{Fd, IoBuf, Packets, MINECRAFT_VERSION, PROTOCOL_VERSION},
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

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
pub fn ingress(
    gametick: ReceiverMut<Gametick>,
    mut fd_lookup: Single<&mut FdLookup>,
    global: Single<&mut Global>,
    mut server: Single<&mut Server>,
    mut players: Fetcher<(
        &mut LoginState,
        &mut DecodeBuffer,
        &mut Packets,
        &Fd,
        Option<&mut FullEntityPose>,
    )>,
    mut io: Single<&mut IoBuf>,
    mut sender: IngressSender,
) {
    let mut gametick = gametick.event;

    // todo why &mut * needed
    let scratch = &mut *gametick.scratch;

    server
        .drain(|event| match event {
            ServerEvent::AddPlayer { fd } => {
                let new_player = sender.spawn();
                sender.insert(new_player, LoginState::Handshake);
                sender.insert(new_player, DecodeBuffer::default());

                sender.insert(new_player, Packets::default());
                sender.insert(new_player, fd);

                fd_lookup.insert(fd, new_player);

                info!("got a player with fd {:?}", fd);
            }
            ServerEvent::RemovePlayer { fd } => {
                let Some(id) = fd_lookup.remove(&fd) else {
                    return;
                };

                sender.despawn(id);

                info!("removed a player with fd {:?}", fd);
            }
            ServerEvent::RecvData { fd, data } => {
                trace!("got data: {data:?}");
                let id = *fd_lookup.get(&fd).expect("player with fd not found");
                let (login_state, decoder, packets, _, mut pose) =
                    players.get_mut(id).expect("player with fd not found");

                decoder.queue_slice(data);

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

                            if let Some(pose) = &mut pose {
                                crate::packets::switch(frame, &global, &mut sender, pose).unwrap();
                            }
                        }
                    }
                }
            }
        })
        .unwrap();

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
