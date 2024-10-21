use std::borrow::Cow;

use rkyv::{Archive, Deserialize, Serialize};

use crate::ChunkPosition;

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
#[rkyv(derive(Debug))]
pub struct UpdatePlayerChunkPositions {
    pub stream: Vec<u64>,
    pub positions: Vec<ChunkPosition>,
}

#[derive(Archive, Deserialize, Serialize, Clone, Copy, PartialEq)]
#[rkyv(derive(Debug))]
pub struct SetReceiveBroadcasts {
    pub stream: u64,
}

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
#[rkyv(derive(Debug))]
pub struct BroadcastGlobal {
    pub exclude: u64,
    pub order: u32,
    pub data: Vec<u8>,
}

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
#[rkyv(derive(Debug))]
pub struct BroadcastLocal {
    pub center: ChunkPosition,
    pub exclude: u64,
    pub order: u32,
    pub data: Vec<u8>,
}

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
#[rkyv(derive(Debug))]
pub struct Multicast {
    pub order: u32,
    pub data: Vec<u8>,
}

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
#[rkyv(derive(Debug))]
pub struct Unicast {
    pub stream: u64,
    pub order: u32,
    pub data: Vec<u8>,
}

#[derive(Archive, Deserialize, Serialize, Clone, Copy, PartialEq)]
#[rkyv(derive(Debug))]
pub struct Flush;

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
#[rkyv(derive(Debug))]
pub enum ServerToProxyMessage {
    UpdatePlayerChunkPositions(UpdatePlayerChunkPositions),
    BroadcastGlobal(BroadcastGlobal),
    BroadcastLocal(BroadcastLocal),
    Multicast(Multicast),
    Unicast(Unicast),
    SetReceiveBroadcasts(SetReceiveBroadcasts),
    Flush(Flush),
}
