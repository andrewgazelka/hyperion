use ser::{types::VarInt, Packet, Readable, EnumReadable};
use uuid::Uuid;

// packet id 0x0
#[derive(Packet, Readable, Debug)]
#[packet(0x0, Handshake)]
pub struct Handshake {
    pub protocol_version: VarInt,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: NextState,
}

// packet id 0x0
#[derive(Packet, Readable, Debug)]
#[packet(0x0, Handshake)]
pub struct StatusRequest;

#[derive(EnumReadable, Debug, Eq, PartialEq)]
pub enum NextState {
    Status = 1,
    Login = 2,
}

// login start
#[derive(Packet, Readable, Debug)]
#[packet(0x0, Handshake)]
pub struct LoginStart {
    pub username: String,
    pub uuid: Uuid,
}
