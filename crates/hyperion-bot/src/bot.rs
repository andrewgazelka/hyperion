use tokio::net::{TcpStream, ToSocketAddrs};
use uuid::Uuid;

mod handshake;

pub struct Bot {
    name: String,
    uuid: Uuid,
    buf: Vec<u8>,
    connection: TcpStream,
}

impl Bot {
    pub async fn new(name: String, uuid: Uuid, addr: impl ToSocketAddrs) -> Self {
        let addr = TcpStream::connect(addr).await.unwrap();

        Self {
            name,
            uuid,
            buf: Vec::new(),
            connection: addr,
        }
    }
}
