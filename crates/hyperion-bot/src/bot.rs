use tokio::net::{TcpStream, ToSocketAddrs};
use tracing::info;
use uuid::Uuid;

mod handshake;

pub struct Bot {
    name: String,
    uuid: Uuid,
    buf: Vec<u8>,
    connection: TcpStream,
}

impl Bot {
    #[tracing::instrument(skip_all, fields(name))]
    pub async fn new(name: String, uuid: Uuid, addr: impl ToSocketAddrs + std::fmt::Display) -> Self {
        info!("connecting to {addr}");
        let addr = TcpStream::connect(addr).await.unwrap();

        Self {
            name,
            uuid,
            buf: Vec::new(),
            connection: addr,
        }
    }
}
