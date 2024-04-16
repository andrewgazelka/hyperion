use evenio::{
    event::{Insert, Receiver, Sender, Spawn},
    fetch::{Fetcher, Single},
};
use evenio::event::Despawn;
use libc::send;
use serde_json::json;
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

use crate::net::{Encoder, Fd, MINECRAFT_VERSION, PROTOCOL_VERSION};

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
pub fn ingress(
    _: Receiver<Gametick>,
    mut fd_lookup: Single<&mut FdLookup>,
    mut global: Single<&mut Global>,
    mut server: Single<&mut Server>,
    mut players: Fetcher<(&mut LoginState, &mut PlayerPacketBuffer, &mut Encoder, &Fd)>,
    mut sender: Sender<(
        Spawn,
        Insert<LoginState>,
        Insert<PlayerPacketBuffer>,
        Insert<Encoder>,
        Insert<Fd>,
        Despawn,
    )>,
) {
    println!("...");
    server.drain(|event| match event {
        ServerEvent::AddPlayer { fd } => {
            println!("add player");
            let new_player = sender.spawn();
            sender.insert(new_player, LoginState::Handshake);
            sender.insert(new_player, PlayerPacketBuffer::default());
            sender.insert(new_player, Encoder::default());
            sender.insert(new_player, fd);

            fd_lookup.insert(fd, new_player);

            global.set_needs_realloc();

            info!("got a player with fd {:?}", fd);
        }
        ServerEvent::RemovePlayer { fd } => {
            let id = fd_lookup.remove(&fd).expect("player with fd not found");
            
            sender.despawn(id);
            
            info!("removed a player with fd {:?}", fd);
        }
        ServerEvent::RecvData { fd, data } => {
            println!("got data: {data:?}");
            let id = *fd_lookup.get(&fd).expect("player with fd not found");
            let (login_state, decoder, encoder, _) =
                players.get_mut(id).expect("player with fd not found");

            decoder.queue_slice(data);

            while let Some(frame) = decoder.try_next_packet().unwrap() {
                match *login_state {
                    LoginState::Handshake => process_handshake(login_state, &frame).unwrap(),
                    LoginState::Status => {
                        process_status(login_state, &frame, encoder, &global).unwrap();
                    }
                    LoginState::Login | LoginState::Play => {}
                }
            }
        }
    });

    server.submit_events();

    let encoders = players.iter_mut().map(|(_, _, encoder, fd)| (encoder, *fd));
    println!("start refreshing buffers");
    server.refresh_buffers(&mut global, encoders);
    println!("done refreshing buffers");
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

fn process_status(
    login_state: &mut LoginState,
    packet: &PacketFrame,
    encoder: &mut Encoder,
    global: &Global,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Status);

    #[allow(clippy::single_match, reason = "todo del")]
    match packet.id {
        packets::status::QueryRequestC2s::ID => {
            let query_request: packets::status::QueryRequestC2s = packet.decode()?;

            println!("query request: {query_request:?}... responding");

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

            println!("query request: {query_request:?}");

            encoder.append(&send, global)?;

            // todo: send response
        }
        _ => {}
    }

    // todo: check version is correct

    Ok(())
}
