//! Communication to a proxy which forwards packets to the players.

use std::{collections::HashMap, io::Cursor, net::SocketAddr, sync::Arc};

use anyhow::bail;
use bytes::{Buf, BytesMut};
use hyperion_proto::{PlayerConnect, PlayerDisconnect, ProxyToServer, ProxyToServerMessage};
use parking_lot::Mutex;
use prost::{encoding::decode_varint, Message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{error, info, warn};

use crate::{component::EgressComm, runtime::AsyncRuntime};

/// This is used
#[derive(Default)]
pub struct ReceiveStateInner {
    /// All players who have recently connected to the server.
    pub player_connect: Vec<PlayerConnect>,
    /// All players who have recently disconnected from the server.
    pub player_disconnect: Vec<PlayerDisconnect>,
    /// A map of stream ids to the corresponding [`BytesMut`] buffers. This represents data from the client to the server.
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

                    loop {
                        let message = match reader.next().await {
                            Ok(message) => message,
                            Err(err) => {
                                error!("failed to process packet {err:?}");
                                return;
                            }
                        };

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

/// A wrapper around [`ReceiveStateInner`]
pub struct ReceiveState(pub Arc<Mutex<ReceiveStateInner>>);

/// Initializes proxy communications.
#[must_use]
pub fn init_proxy_comms(tasks: &AsyncRuntime, socket: SocketAddr) -> (ReceiveState, EgressComm) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let shared = Arc::new(Mutex::new(ReceiveStateInner::default()));

    tasks.block_on(async {
        inner(socket, rx, shared.clone()).await;
    });

    (ReceiveState(shared), EgressComm::from(tx))
}

#[derive(Debug)]
struct ProxyReader {
    server_read: tokio::net::tcp::OwnedReadHalf,
    buffer: BytesMut,
}

impl ProxyReader {
    pub fn new(server_read: tokio::net::tcp::OwnedReadHalf) -> Self {
        Self {
            server_read,
            buffer: BytesMut::with_capacity(1024 * 1024),
        }
    }

    pub async fn next(&mut self) -> anyhow::Result<ProxyToServerMessage> {
        let message = self.next_server_packet().await?;
        Ok(message)
    }

    // #[instrument]
    async fn next_server_packet(&mut self) -> anyhow::Result<ProxyToServerMessage> {
        let len = loop {
            if !self.buffer.is_empty() {
                let mut cursor = Cursor::new(&self.buffer);

                // tood: handle invalid varint
                if let Ok(len) = decode_varint(&mut cursor) {
                    self.buffer.advance(cursor.position() as usize);
                    break len as usize;
                }
            }

            self.server_read.read_buf(&mut self.buffer).await?;
        };

        // todo: this needed?
        self.buffer.reserve(len);

        while self.buffer.len() < len {
            self.server_read.read_buf(&mut self.buffer).await?;
        }

        let mut buffer = self.buffer.split_to(len);

        let Ok(message) = ProxyToServer::decode(&mut buffer) else {
            bail!("Failed to decode ProxyToServerMessage from {:?}", buffer);
        };

        assert!(buffer.is_empty());

        let Some(message) = message.proxy_to_server_message else {
            bail!("No message in ServerToProxy message");
        };

        Ok(message)
    }
}
