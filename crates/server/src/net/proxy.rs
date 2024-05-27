use std::io::Cursor;

use anyhow::{bail, Context};
use bytes::BytesMut;
use hyperion_proto::{PlayerConnect, PlayerDisconnect, ProxyToServer, ProxyToServerMessage};
use parking_lot::Mutex;
use prost::{encoding::decode_varint, Message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::instrument;

use crate::components::chunks::Tasks;

pub struct ReceiveState {
    pub player_connect: Vec<PlayerConnect>,
    pub player_disconnect: Vec<PlayerDisconnect>,
    pub packets: targeted_bulk::TargetedEvents<bytes::Bytes, u64>,
}

pub struct ProxyComms {
    pub tx: tokio::sync::mpsc::UnboundedSender<bytes::Bytes>,

    /// Should we use std Mutex instead? think we will need to benchmark
    ///
    /// <https://users.rust-lang.org/t/which-mutex-to-use-parking-lot-or-std-sync/85060/6>
    pub shared: Mutex<ReceiveState>,
}

fn init_proxy_comms(
    tasks: &Tasks,
    socket: tokio::net::TcpStream,
    mut server_to_proxy: tokio::sync::mpsc::UnboundedReceiver<bytes::Bytes>,
    shared: Mutex<ReceiveState>,
) {
    let (read, mut write) = socket.into_split();
    let read = tokio::io::BufReader::new(read);

    tasks.spawn(async move {
        while let Some(bytes) = server_to_proxy.recv().await {
            write.write_all(&bytes).await.unwrap();
        }
    });

    tasks.spawn(async move {
        let mut reader = ProxyReader::new(read);

        loop {
            let message = reader.next().await.unwrap();

            match message {
                ProxyToServerMessage::PlayerConnect(message) => {
                    shared.lock().player_connect.push(message);
                }
                ProxyToServerMessage::PlayerDisconnect(message) => {
                    shared.lock().player_disconnect.push(message);
                }
                ProxyToServerMessage::PlayerPackets(message) => {
                    shared
                        .lock()
                        .packets
                        .push_exclusive(message.stream, message.data);
                }
            }
        }
    });
}

#[derive(Debug)]
struct ProxyReader {
    server_read: tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>,
    buffer: BytesMut,
}

impl ProxyReader {
    pub fn new(server_read: tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>) -> Self {
        Self {
            server_read,
            buffer: BytesMut::with_capacity(1024),
        }
    }

    #[instrument]
    pub async fn next(&mut self) -> anyhow::Result<ProxyToServerMessage> {
        let len = self.read_len().await?;
        let message = self.next_server_packet(len).await?;
        Ok(message)
    }

    #[instrument]
    async fn read_len(&mut self) -> anyhow::Result<usize> {
        let mut vint = [0u8; 4];
        let mut i = 0;
        let len = loop {
            let byte = self.server_read.read_u8().await?;
            let to_set = vint
                .get_mut(i)
                .context("Failed to get mutable reference to byte in vint")?;
            *to_set = byte;
            let mut cursor = Cursor::new(vint.as_slice());
            if let Ok(len) = decode_varint(&mut cursor) {
                break len;
            }
            i += 1;
        };
        Ok(usize::try_from(len).expect("Failed to convert varint to usize"))
    }

    #[instrument]
    async fn next_server_packet(&mut self, len: usize) -> anyhow::Result<ProxyToServerMessage> {
        // todo: this needed?
        if self.buffer.len() < len {
            self.buffer.resize(len, 0);
        }

        let slice = &mut self.buffer[..len];
        self.server_read.read_exact(slice).await?;

        let Ok(message) = ProxyToServer::decode(&mut self.buffer) else {
            bail!("Failed to decode ServerToProxy message");
        };

        let Some(message) = message.proxy_to_server_message else {
            bail!("No message in ServerToProxy message");
        };

        Ok(message)
    }
}
