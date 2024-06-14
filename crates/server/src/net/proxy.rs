use std::{collections::HashMap, io::Cursor, net::SocketAddr, sync::Arc};

use anyhow::{bail, Context};
use bytes::BytesMut;
use hyperion_proto::{PlayerConnect, PlayerDisconnect, ProxyToServer, ProxyToServerMessage};
use parking_lot::Mutex;
use prost::{encoding::decode_varint, Message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{error, info, warn};

use crate::{component::EgressComm, tasks::Tasks};

#[derive(Default)]
pub struct ReceiveStateInner {
    pub player_connect: Vec<PlayerConnect>,
    pub player_disconnect: Vec<PlayerDisconnect>,
    pub packets: HashMap<u64, BytesMut>,
}

async fn inner(
    socket: SocketAddr,
    mut server_to_proxy: tokio::sync::mpsc::UnboundedReceiver<bytes::Bytes>,
    shared: Arc<Mutex<ReceiveStateInner>>,
) {
    let listener = tokio::net::TcpListener::bind(socket).await.unwrap();

    tokio::spawn(
        async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();

                let addr = socket.peer_addr().unwrap();

                info!("Proxy connection established on {addr}");

                let shared = shared.clone();

                let (read, mut write) = socket.into_split();
                let read = tokio::io::BufReader::new(read);

                let fst = tokio::spawn(async move {
                    while let Some(bytes) = server_to_proxy.recv().await {
                        if write.write_all(&bytes).await.is_err() {
                            error!("error writing to proxy");
                            return server_to_proxy;
                        }
                    }

                    warn!("proxy shut down");

                    server_to_proxy
                });

                tokio::spawn(async move {
                    let mut reader = ProxyReader::new(read);

                    while let Ok(message) = reader.next().await {
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
                                    .entry(message.stream)
                                    .or_default()
                                    // todo: remove extra allocations
                                    .extend_from_slice(&message.data);
                            }
                        }
                    }
                    error!("error reading from proxy");
                });

                // todo: handle player disconnects on proxy shut down
                // Ideally, we should design for there being multiple proxies,
                // and all proxies should store all the players on them.
                // Then we can disconnect all those players related to that proxy.
                server_to_proxy = fst.await.unwrap();
            }
        }, // .instrument(info_span!("proxy reader")),
    );
}

pub struct ReceiveState(pub Arc<Mutex<ReceiveStateInner>>);

pub fn init_proxy_comms(tasks: &Tasks, socket: SocketAddr) -> (ReceiveState, EgressComm) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let shared = Arc::new(Mutex::new(ReceiveStateInner::default()));

    tasks.block_on(async {
        inner(socket, rx, shared.clone()).await;
    });

    (ReceiveState(shared), EgressComm::from(tx))
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

    // #[instrument]
    pub async fn next(&mut self) -> anyhow::Result<ProxyToServerMessage> {
        let len = self.read_len().await?;
        let message = self.next_server_packet(len).await?;
        Ok(message)
    }

    // #[instrument]
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

    // #[instrument]
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
