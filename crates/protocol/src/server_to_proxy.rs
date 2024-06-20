include!(concat!(env!("OUT_DIR"), "/server_to_proxy.rs"));

impl From<UpdatePlayerChunkPositions> for ServerToProxyMessage {
    fn from(message: UpdatePlayerChunkPositions) -> Self {
        Self::UpdatePlayerChunkPositions(message)
    }
}

impl From<BroadcastGlobal> for ServerToProxyMessage {
    fn from(message: BroadcastGlobal) -> Self {
        Self::BroadcastGlobal(message)
    }
}

impl From<BroadcastLocal> for ServerToProxyMessage {
    fn from(message: BroadcastLocal) -> Self {
        Self::BroadcastLocal(message)
    }
}

impl From<Multicast> for ServerToProxyMessage {
    fn from(message: Multicast) -> Self {
        Self::Multicast(message)
    }
}

impl From<Unicast> for ServerToProxyMessage {
    fn from(message: Unicast) -> Self {
        Self::Unicast(message)
    }
}

impl From<SetReceiveBroadcasts> for ServerToProxyMessage {
    fn from(message: SetReceiveBroadcasts) -> Self {
        Self::SetReceiveBroadcasts(message)
    }
}

impl From<Flush> for ServerToProxyMessage {
    fn from(message: Flush) -> Self {
        Self::Flush(message)
    }
}

impl<T: Into<ServerToProxyMessage>> From<T> for ServerToProxy {
    fn from(message: T) -> Self {
        Self {
            server_to_proxy_message: Some(message.into()),
        }
    }
}

pub use server_to_proxy::ServerToProxyMessage;

impl Copy for SetReceiveBroadcasts {}
impl Copy for Flush {}
