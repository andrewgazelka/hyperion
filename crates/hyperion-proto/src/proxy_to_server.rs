use rkyv::{Archive, Deserialize, Serialize};

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq, Debug)]
#[rkyv(derive(Debug))]
pub struct PlayerPackets {
    pub stream: u64,
    pub data: Vec<u8>,
}

#[derive(Archive, Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
#[rkyv(derive(Debug))]
pub struct PlayerConnect {
    pub stream: u64,
}

#[derive(Archive, Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
#[rkyv(derive(Debug))]
pub struct PlayerDisconnect {
    pub stream: u64,
}

#[derive(Archive, Deserialize, Serialize, Clone, PartialEq, Debug)]
#[rkyv(derive(Debug))]
pub enum ProxyToServerMessage {
    PlayerConnect(PlayerConnect),
    PlayerDisconnect(PlayerDisconnect),
    PlayerPackets(PlayerPackets),
}
