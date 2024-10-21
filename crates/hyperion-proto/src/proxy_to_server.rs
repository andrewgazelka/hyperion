use rkyv::{with::InlineAsBox, Archive, Deserialize, Serialize};

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq, Debug)]
pub struct PlayerPackets<'a> {
    pub stream: u64,

    #[rkyv(with = InlineAsBox)]
    pub data: &'a [u8],
}

#[derive(Archive, Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
pub struct PlayerConnect {
    pub stream: u64,
}

#[derive(Archive, Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
pub struct PlayerDisconnect {
    pub stream: u64,
}

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq, Debug)]
pub enum ProxyToServerMessage<'a> {
    PlayerConnect(PlayerConnect),
    PlayerDisconnect(PlayerDisconnect),
    PlayerPackets(PlayerPackets<'a>),
}
