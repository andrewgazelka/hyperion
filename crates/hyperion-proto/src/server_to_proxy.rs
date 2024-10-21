use rkyv::{with::InlineAsBox, Archive, Deserialize, Serialize};

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
pub struct BroadcastGlobal<'a> {
    pub exclude: u64,
    pub order: u32,

    #[rkyv(with = InlineAsBox)]
    pub data: &'a [u8],
}

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
// #[rkyv(derive(Debug))]
pub struct BroadcastLocal<'a> {
    pub center: ChunkPosition,
    pub exclude: u64,
    pub order: u32,

    #[rkyv(with = InlineAsBox)]
    pub data: &'a [u8],
}

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
// #[rkyv(derive(Debug))]
pub struct Multicast<'a> {
    pub order: u32,
    #[rkyv(with = InlineAsBox)]
    pub data: &'a [u8],
}

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
// #[rkyv(derive(Debug))]
pub struct Unicast<'a> {
    pub stream: u64,
    pub order: u32,

    #[rkyv(with = InlineAsBox)]
    pub data: &'a [u8],
}

#[derive(Archive, Deserialize, Serialize, Clone, Copy, PartialEq)]
#[rkyv(derive(Debug))]
pub struct Flush;

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq)]
// #[rkyv(derive(Debug))]
pub enum ServerToProxyMessage<'a> {
    UpdatePlayerChunkPositions(UpdatePlayerChunkPositions),
    BroadcastGlobal(BroadcastGlobal<'a>),
    BroadcastLocal(BroadcastLocal<'a>),
    Multicast(Multicast<'a>),
    Unicast(Unicast<'a>),
    SetReceiveBroadcasts(SetReceiveBroadcasts),
    Flush(Flush),
}
