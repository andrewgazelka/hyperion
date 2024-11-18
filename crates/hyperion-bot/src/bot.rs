use bytes::BytesMut;
use tokio::net::{TcpStream, ToSocketAddrs};
use tracing::info;
use uuid::Uuid;
use valence_protocol::{PacketDecoder, PacketEncoder};

mod handshake;

pub struct Bot {
    name: String,
    uuid: Uuid,
    connection: TcpStream,
    encoder: PacketEncoder,
    decoder: PacketDecoder,
    decode_buf: BytesMut,
}

impl Bot {
    #[tracing::instrument(skip_all, fields(name))]
    pub async fn new(
        name: String,
        uuid: Uuid,
        addr: impl ToSocketAddrs + std::fmt::Display,
    ) -> Self {
        info!("connecting to {addr}");
        let addr = TcpStream::connect(addr).await.unwrap();

        let encoder = PacketEncoder::default();
        let decoder = PacketDecoder::default();

        let decode_buf = BytesMut::with_capacity(1024 * 1024); // 1 MiB

        Self {
            name,
            uuid,
            connection: addr,
            encoder,
            decoder,
            decode_buf,
        }
    }
}
