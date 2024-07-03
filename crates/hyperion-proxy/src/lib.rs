#![feature(maybe_uninit_slice)]
#![feature(allocator_api)]
#![feature(io_slice_advance)]
#![feature(let_chains)]
#![allow(
    clippy::redundant_pub_crate,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::missing_panics_doc,
    clippy::module_inception,
    clippy::future_not_send
)]

use std::{
    fmt::Debug,
    io::Cursor,
    sync::{atomic::AtomicBool, Arc},
};

use anyhow::{bail, Context};
use hyperion_proto::{ServerToProxy, ServerToProxyMessage};
use prost::{encoding::decode_varint, Message};
use tokio::{
    io::{AsyncReadExt, BufReader},
    net::{TcpListener, TcpStream, ToSocketAddrs},
};
use tracing::{error, info, instrument, trace, trace_span, Instrument};

use crate::{
    cache::BufferedEgress,
    data::{PlayerHandle, PlayerRegistry},
    egress::Egress,
    player::launch_player,
    server_sender::launch_server_writer,
};

const DEFAULT_BUFFER_SIZE: usize = 4 * 1024;

pub mod cache;
pub mod data;
pub mod egress;
pub mod player;
pub mod server_sender;

#[tracing::instrument(skip_all)]
pub async fn connect(addr: impl ToSocketAddrs + Debug + Clone) -> TcpStream {
    loop {
        if let Ok(stream) = TcpStream::connect(addr.clone()).await {
            return stream;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[tracing::instrument(skip_all)]
pub async fn run_proxy(
    proxy_addr: impl ToSocketAddrs + Debug + Clone,
    server_addr: impl ToSocketAddrs + Debug + Clone,
) -> anyhow::Result<()> {
    let mut listener = TcpListener::bind(proxy_addr).await?;

    loop {
        let server_socket = connect(server_addr.clone()).await;
        if let Err(e) = connect_to_server_and_run_proxy(&mut listener, server_socket).await {
            error!("Error connecting to server: {e:?}");
        }
    }
}

#[instrument(level = "trace")]
async fn connect_to_server_and_run_proxy(
    listener: &mut TcpListener,
    server_socket: TcpStream,
) -> anyhow::Result<()> {
    let (server_read, server_write) = server_socket.into_split();
    let server_sender = launch_server_writer(server_write);
    let mut reader = ServerReader::new(BufReader::new(server_read));
    let player_registry = Arc::new(PlayerRegistry::default());

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn({
        let data = player_registry.clone();
        async move {
            let egress = Egress::new(data);
            let egress = Arc::new(egress);
            let mut egress = BufferedEgress::new(egress);

            loop {
                match reader.next().await {
                    Ok(packet) => egress.handle_packet(packet),
                    Err(e) => {
                        error!(
                            "Error reading next packet: {e:?}. Are you connected to a valid \
                             hyperion server? If you are connected to a vanilla server, \
                             hyperion-proxy will not work."
                        );
                        break;
                    }
                }
            }

            info!("Sending shutdown to all players");

            shutdown_tx.send(true).unwrap();
        }
        .instrument(trace_span!("server_reader_loop"))
    });

    loop {
        let mut shutdown_rx = shutdown_rx.clone();
        let socket: TcpStream = tokio::select! {
            _ = shutdown_rx.wait_for(|value| *value) => {
                return Ok(())
            }
            Ok((socket, _)) = listener.accept() => {
                // todo: think there are some unhandled cases here
                socket
            }
        };

        let (tx, rx) = tokio::sync::mpsc::channel(1024);
        let id = player_registry.write().unwrap().insert(PlayerHandle {
            writer: tx,
            can_receive_broadcasts: AtomicBool::new(false),
        });

        info!("got player with id {id:?}");

        launch_player(
            socket,
            shutdown_rx.clone(),
            id,
            rx,
            server_sender.clone(),
            player_registry.clone(),
        );
    }
}

struct ServerReader {
    server_read: BufReader<tokio::net::tcp::OwnedReadHalf>,
    buffer: Vec<u8>,
}

impl Debug for ServerReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerReader").finish()
    }
}

impl ServerReader {
    #[instrument(level = "trace")]
    pub fn new(server_read: BufReader<tokio::net::tcp::OwnedReadHalf>) -> Self {
        Self {
            server_read,
            buffer: Vec::with_capacity(DEFAULT_BUFFER_SIZE),
        }
    }

    #[instrument(level = "trace")]
    pub async fn next(&mut self) -> anyhow::Result<ServerToProxyMessage> {
        let len = self.read_varint().await?;

        trace!("Received packet of length {len}");

        let message = self.next_server_packet(len).await?;
        Ok(message)
    }

    #[instrument(level = "trace")]
    async fn read_varint(&mut self) -> anyhow::Result<usize> {
        let mut vint = [0u8; 4];
        let mut i = 0;
        let len = loop {
            let byte = self.server_read.read_u8().await?;

            let to_set = vint
                .get_mut(i)
                .context("Failed to get mutable reference to byte in vint")?;
            *to_set = byte;
            let mut cursor = Cursor::new(&vint[..=i]);
            if let Ok(len) = decode_varint(&mut cursor) {
                break len;
            }
            i += 1;
        };
        Ok(usize::try_from(len).expect("Failed to convert varint to usize"))
    }

    #[instrument(level = "trace")]
    async fn next_server_packet(&mut self, len: usize) -> anyhow::Result<ServerToProxyMessage> {
        if self.buffer.len() < len {
            self.buffer.resize(len, 0);
        }
        let slice = &mut self.buffer[..len];
        self.server_read.read_exact(slice).await?;
        let mut cursor = Cursor::new(slice);
        let Ok(message) = ServerToProxy::decode(&mut cursor) else {
            bail!("Failed to decode ServerToProxy message");
        };
        let Some(message) = message.server_to_proxy_message else {
            bail!("No message in ServerToProxy message");
        };
        Ok(message)
    }
}
