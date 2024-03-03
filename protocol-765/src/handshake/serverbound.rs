use ser::{types::VarInt, EnumReadable, EnumWritable, Packet, Readable, Writable};
use uuid::Uuid;

// packet id 0x0
#[derive(Packet, Writable, Readable, Debug)]
#[packet(0x0)]
pub struct Handshake<'a> {
    pub protocol_version: VarInt,
    pub server_address: &'a str,
    pub server_port: u16,
    pub next_state: NextState,
}

// packet id 0x0
#[derive(Packet, Writable, Readable, Debug)]
#[packet(0x0)]
pub struct StatusRequest;

#[derive(EnumReadable, EnumWritable, Debug, Eq, PartialEq, Copy, Clone)]
pub enum NextState {
    Status = 1,
    Login = 2,
}

// login start
#[derive(Packet, Readable, Debug)]
#[packet(0x0)]
pub struct LoginStart<'a> {
    pub username: &'a str,
    pub uuid: Uuid,
}

#[derive(Packet, Writable, Readable, Debug)]
#[packet(0x1)]
pub struct Ping {
    pub payload: i64,
}

// Login Acknowledged
// Acknowledgement to the Login Success packet sent by the server.
//
// Packet ID	State	Bound To	Field Name	Field Type	Notes
// 0x03	Login	Server	no fields
#[derive(Packet, Writable, Readable, Debug)]
#[packet(0x3)]
pub struct LoginAcknowledged;
