use valence_protocol::packets;

use crate::{
    components::{LoginState, LoginStatePendingC2s, LoginStatePendingS2c},
    net::{encoder::append_packet_without_compression, MINECRAFT_VERSION, PROTOCOL_VERSION},
};

pub fn generate_status_packets(buffer: &mut [u8], login_state: &mut LoginState) -> anyhow::Result<usize> {
    match login_state {
        LoginState::PendingS2c(state) => match state {
            LoginStatePendingS2c::StatusResponse => {
                // TODO: Cache status response packet
                // https://wiki.vg/Server_List_Ping#Response
                let json = serde_json::json!({
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

                let json = serde_json::to_string_pretty(&json).unwrap();

                let send = packets::status::QueryResponseS2c { json: &json };
                let bytes_written = append_packet_without_compression(&send, buffer)?;
                *login_state = LoginState::PendingC2s(LoginStatePendingC2s::StatusPing);
                Ok(bytes_written)
            },
            LoginStatePendingS2c::StatusPong { payload } => {
                let send = packets::status::QueryPongS2c { payload: *payload };
                let bytes_written = append_packet_without_compression(&send, buffer)?;
                // TODO: Actually close connection instead of doing this
                *login_state = LoginState::PendingC2s(LoginStatePendingC2s::Handshake);
                Ok(bytes_written)
            },
        },
        _ => Ok(0)
    }
}
