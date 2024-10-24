#![feature(maybe_uninit_slice)]
#![feature(allocator_api)]
#![feature(let_chains)]
#![feature(coroutines)]
#![feature(never_type)]
#![feature(iter_from_coroutine)]
#![feature(stmt_expr_attributes)]
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

use std::{fmt::Debug, sync::atomic::AtomicBool};

use anyhow::Context;
use colored::Colorize;
use hyperion_proto::{ArchivedServerToProxyMessage, ChunkPosition};
use rustc_hash::FxBuildHasher;
use tokio::{
    io::{AsyncReadExt, BufReader},
    net::{TcpStream, ToSocketAddrs},
};
use tokio_util::net::Listener;
use tracing::{debug, error, info, info_span, instrument, trace, warn, Instrument};

use crate::{
    cache::BufferedEgress, data::PlayerHandle, egress::Egress, player::initiate_player_connection,
    server_sender::launch_server_writer,
};

const DEFAULT_BUFFER_SIZE: usize = 4 * 1024;

pub mod cache;
pub mod data;
pub mod egress;
pub mod player;
pub mod server_sender;
pub mod util;

#[tracing::instrument(level = "trace", skip_all)]
pub async fn connect(addr: impl ToSocketAddrs + Debug + Clone) -> TcpStream {
    loop {
        if let Ok(stream) = TcpStream::connect(addr.clone()).await {
            return stream;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[derive(Debug, PartialEq)]
enum ShutdownType {
    Reconnect,
    Full,
}

#[tracing::instrument(level = "trace", skip_all)]
pub async fn run_proxy(
    mut listener: impl HyperionListener,
    server_addr: impl ToSocketAddrs + Debug + Clone,
) -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(None);

    tokio::task::Builder::new()
        .name("ctrl-c")
        .spawn({
            let shutdown_tx = shutdown_tx.clone();
            async move {
                tokio::signal::ctrl_c().await.unwrap();
                warn!("ctrl-c received, shutting down");
                shutdown_tx.send(Some(ShutdownType::Full)).unwrap();
            }
        })
        .unwrap();

    loop {
        let mut shutdown_rx2 = shutdown_rx.clone();

        if *shutdown_rx2.borrow() == Some(ShutdownType::Full) {
            break Ok(());
        }

        tokio::select! {
            _ = shutdown_rx2.wait_for(|value| *value == Some(ShutdownType::Full)) => {
                warn!("Received shutdown signal, exiting proxy loop");
                break Ok(());
            }
            () = async {

                // clear shutdown channel
                shutdown_tx.send(None).unwrap();

                let binding_help = "~ Make sure the event server is running".dimmed();
                info!("⏳ Binding to server... {binding_help}");

                let server_socket = connect(server_addr.clone()).await;
                server_socket.set_nodelay(true).unwrap();

                if let Err(e) = connect_to_server_and_run_proxy(&mut listener, server_socket, shutdown_rx.clone(), shutdown_tx.clone()).await {
                    error!("Error connecting to server: {e:?}");
                }


            } => {}
        }
    }
}

#[tracing::instrument(level = "trace", skip_all)]
async fn connect_to_server_and_run_proxy(
    listener: &mut impl HyperionListener,
    server_socket: TcpStream,
    shutdown_rx: tokio::sync::watch::Receiver<Option<ShutdownType>>,
    shutdown_tx: tokio::sync::watch::Sender<Option<ShutdownType>>,
) -> anyhow::Result<()> {
    info!("🔗 Connected to server, accepting connections");
    let (server_read, server_write) = server_socket.into_split();
    let server_sender = launch_server_writer(server_write);

    let player_registry = papaya::HashMap::default();
    let player_registry: &'static papaya::HashMap<u64, PlayerHandle, FxBuildHasher> =
        Box::leak(Box::new(player_registry));

    let player_positions = papaya::HashMap::default();
    let player_positions: &'static papaya::HashMap<u64, ChunkPosition, FxBuildHasher> =
        Box::leak(Box::new(player_positions));

    let egress = Egress::new(player_registry, player_positions);

    let egress = BufferedEgress::new(egress);

    let mut handler = IngressHandler::new(BufReader::new(server_read), egress);

    tokio::task::Builder::new()
        .name("s2prox")
        .spawn({
        let mut shutdown_rx = shutdown_rx.clone();

        async move {

            loop {
                tokio::select! {
                    _ = shutdown_rx.wait_for(Option::is_some) => return,
                    result = handler.handle_next() => {
                        match result {
                            Ok(()) => {},
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
                }
            }

            debug!("Sending shutdown to all players");

            shutdown_tx.send(Some(ShutdownType::Reconnect)).unwrap();
        }
        .instrument(info_span!("server_reader_loop"))
    }).unwrap();

    // 0 is reserved for "None" value
    let mut player_id_on = 1;

    loop {
        let mut shutdown_rx = shutdown_rx.clone();
        let socket = tokio::select! {
            _ = shutdown_rx.wait_for(Option::is_some) => {
                return Ok(())
            }
            Ok((socket, addr)) = listener.accept() => {
                info!("New client connection from {addr:?}");
                socket
            }
        };

        let registry = player_registry.pin();

        let (tx, rx) = kanal::bounded_async(1024);
        registry.insert(player_id_on, PlayerHandle {
            writer: tx,
            can_receive_broadcasts: AtomicBool::new(false),
        });

        // todo: some SlotMap like thing
        debug!("got player with id {player_id_on:?}");

        initiate_player_connection(
            socket,
            shutdown_rx.clone(),
            player_id_on,
            rx,
            server_sender.clone(),
            player_registry,
            player_positions,
        );

        player_id_on += 1;
    }
}

struct IngressHandler {
    server_read: BufReader<tokio::net::tcp::OwnedReadHalf>,
    buffer: Vec<u8>,
    egress: BufferedEgress,
}

impl Debug for IngressHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerReader").finish()
    }
}

impl IngressHandler {
    pub fn new(
        server_read: BufReader<tokio::net::tcp::OwnedReadHalf>,
        egress: BufferedEgress,
    ) -> Self {
        Self {
            server_read,
            egress,
            buffer: Vec::with_capacity(DEFAULT_BUFFER_SIZE),
        }
    }

    // #[instrument(level = "info", skip_all, name = "ServerReader::next")]
    pub async fn handle_next(&mut self) -> anyhow::Result<()> {
        let len = self.read_len().await?;
        let len = usize::try_from(len).context("Failed to convert len to usize")?;

        debug_assert!(len <= 1_000_000);

        trace!("Received packet of length {len}");

        self.handle_next_server_packet(len).await
    }

    #[instrument(level = "trace")]
    async fn read_len(&mut self) -> anyhow::Result<u64> {
        self.server_read
            .read_u64()
            .await
            .context("Failed to read int")
    }

    #[instrument(level = "trace")]
    async fn handle_next_server_packet(&mut self, len: usize) -> anyhow::Result<()> {
        // [A]
        if self.buffer.len() < len {
            self.buffer.resize(len, 0);
        }

        #[expect(
            clippy::indexing_slicing,
            reason = "we already verified in [A] that length of buffer is at least {len}"
        )]
        let slice = &mut self.buffer[..len];
        self.server_read.read_exact(slice).await?;

        let result = unsafe { rkyv::access_unchecked::<ArchivedServerToProxyMessage<'_>>(slice) };

        self.egress.handle_packet(result);

        Ok(())
    }
}

trait HyperionListener: Listener<Io: Send, Addr: Debug> + 'static {}

impl<L: Listener<Io: Send, Addr: Debug> + 'static> HyperionListener for L {}
