use evenio::{
    entity::EntityId,
    event::{Insert, Receiver, Sender, Spawn},
    fetch::{Fetcher, Single},
};
use libc::send;
use tracing::{info, instrument, trace, warn};
use valence_protocol::{packets, Decode, PacketDecoder, VarInt};

use crate::{
    global::Global,
    net::{Server, ServerDef, ServerEvent},
    singleton::{
        fd_lookup::FdLookup, player_id_lookup::PlayerIdLookup, player_uuid_lookup::PlayerUuidLookup,
    },
    system::IngressSender,
    ConnectionRef, FullEntityPose, Gametick, LoginState, Player,
};

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
pub fn ingress(
    _: Receiver<Gametick>,
    mut fd_lookup: Single<&mut FdLookup>,
    mut server: Single<&mut Server>,
    fetcher: Fetcher<&LoginState>,
    mut sender: Sender<(Spawn, Insert<LoginState>)>,
) {
    server.drain(|event| match event {
        ServerEvent::AddPlayer { fd } => {
            let new_player = sender.spawn();
            sender.insert(new_player, LoginState::Handshake);

            fd_lookup.insert(fd, new_player);

            info!("got a player with fd {:?}", fd);
        }
        ServerEvent::RemovePlayer { fd } => {
            info!("removed a player with fd {:?}", fd);
        }
        ServerEvent::RecvData { fd, mut data } => {
            let id = *fd_lookup.get(&fd).expect("player with fd not found");
            let login_state = fetcher.get(id).expect("player with fd not found");

            println!("data: {data:?}");

            let data = &mut data;

            let frame = to_frame(data).unwrap();
            let handhake: packets::handshaking::HandshakeC2s = frame.decode().unwrap();
            
            if data.len() > 0 {
                println!("data: {data:?}");
            }

            println!("{:?}", handhake);

            info!("got data from player with fd {:?}: {data:?}", fd);
            info!("login state: {login_state:?}");
        }
    });

    server.submit_events();
}

struct PacketFrameSlice<'a> {
    id: i32,
    data: &'a [u8],
}

impl<'a> PacketFrameSlice<'a> {
    fn decode<T: Decode<'a>>(&self) -> anyhow::Result<T> {
        let mut input = self.data;
        let result = T::decode(&mut input)?;
        if input.is_empty() {
            Ok(result)
        } else {
            Err(anyhow::anyhow!("expected end of data"))
        }
    }
}

fn to_frame<'a>(input: &mut &'a [u8]) -> anyhow::Result<PacketFrameSlice<'a>> {
    let len = VarInt::decode(input)?;

    let (mut process, extra) = input.split_at(len.0 as usize);
    *input = extra;

    let id = VarInt::decode(&mut process)?;

    Ok(PacketFrameSlice {
        id: id.0,
        data: process,
    })
}
