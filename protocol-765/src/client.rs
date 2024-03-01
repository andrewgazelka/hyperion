use ser::{types::VarInt, Packet, Readable};

// packet id 0x0
#[derive(Packet, Readable, Debug)]
#[packet(0, Handshake)]
pub struct Handshake {
    pub protocol_version: VarInt,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: VarInt,
}
