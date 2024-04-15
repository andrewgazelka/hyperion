use anyhow::bail;
use derive_more::{Deref, DerefMut};
use evenio::{
    component::Component,
    entity::EntityId,
    event::{Insert, Receiver, Sender, Spawn},
    fetch::{Fetcher, Single},
};
use libc::send;
use tracing::{field::debug, info, instrument, trace, warn};
use valence_protocol::{
    decode::PacketFrame, packets, packets::handshaking::handshake_c2s::HandshakeNextState,
    var_int::VarIntDecodeError, Decode, Packet, PacketDecoder, VarInt,
};

use crate::{
    global::Global,
    net::{Server, ServerDef, ServerEvent},
    singleton::{
        fd_lookup::FdLookup, player_id_lookup::PlayerIdLookup, player_uuid_lookup::PlayerUuidLookup,
    },
    system::IngressSender,
    ConnectionRef, FullEntityPose, Gametick, LoginState, Player,
};

mod player_packet_buffer;
pub use player_packet_buffer::PlayerPacketBuffer;

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
pub fn ingress(
    _: Receiver<Gametick>,
    mut fd_lookup: Single<&mut FdLookup>,
    mut server: Single<&mut Server>,
    mut fetcher: Fetcher<(&mut LoginState, &mut PlayerPacketBuffer)>,
    mut sender: Sender<(Spawn, Insert<LoginState>, Insert<PlayerPacketBuffer>)>,
) {
    server.drain(|event| match event {
        ServerEvent::AddPlayer { fd } => {
            let new_player = sender.spawn();
            sender.insert(new_player, LoginState::Handshake);
            sender.insert(new_player, PlayerPacketBuffer::default());

            fd_lookup.insert(fd, new_player);

            info!("got a player with fd {:?}", fd);
        }
        ServerEvent::RemovePlayer { fd } => {
            info!("removed a player with fd {:?}", fd);
        }
        ServerEvent::RecvData { fd, data } => {
            let id = *fd_lookup.get(&fd).expect("player with fd not found");
            let (login_state, decoder) = fetcher.get_mut(id).expect("player with fd not found");

            decoder.queue_slice(data);

            while let Some(frame) = decoder.try_next_packet().unwrap() {
                match *login_state {
                    LoginState::Handshake => process_handshake(login_state, &frame).unwrap(),
                    LoginState::Status => process_status(login_state, &frame).unwrap(),
                    LoginState::Login => {}
                    LoginState::Play => {}
                }
            }
        }
    });

    server.submit_events();
}

fn process_handshake(login_state: &mut LoginState, packet: &PacketFrame) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Handshake);

    let handshake: packets::handshaking::HandshakeC2s = packet.decode()?;

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

fn process_status(login_state: &mut LoginState, packet: &PacketFrame) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Status);

    match packet.id {
        packets::status::QueryRequestC2s::ID => {
            let query_request: packets::status::QueryRequestC2s = packet.decode()?;

            println!("query request: {query_request:?}");

            // todo: send response
        }
        _ => {}
    }

    // todo: check version is correct

    Ok(())
}
