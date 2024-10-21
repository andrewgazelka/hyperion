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
use hyperion_proto::ArchivedServerToProxyMessage;
use rustc_hash::FxBuildHasher;
use tokio::{
    io::{AsyncReadExt, BufReader},
    net::{TcpListener, TcpStream, ToSocketAddrs},
};
use tracing::{debug, error, info_span, instrument, trace, Instrument};

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

#[tracing::instrument(level = "trace", skip_all)]
pub async fn run_proxy(
    proxy_addr: impl ToSocketAddrs + Debug + Clone,
    server_addr: impl ToSocketAddrs + Debug + Clone,
) -> anyhow::Result<()> {
    let mut listener = TcpListener::bind(proxy_addr).await?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::task::Builder::new()
        .name("ctrl-c")
        .spawn({
            let shutdown_tx = shutdown_tx.clone();
            async move {
                tokio::signal::ctrl_c().await.unwrap();
                println!("ctrl-c received, shutting down");
                shutdown_tx.send(true).unwrap();
            }
        })
        .unwrap();

    loop {
        let mut shutdown_rx2 = shutdown_rx.clone();
        tokio::select! {
            _ = shutdown_rx2.changed() => {
                println!("Received shutdown signal, exiting proxy loop");
                break Ok(());
            }
            () = async {
                let server_socket = connect(server_addr.clone()).await;
                if let Err(e) = connect_to_server_and_run_proxy(&mut listener, server_socket, shutdown_rx.clone(), shutdown_tx.clone()).await {
                    error!("Error connecting to server: {e:?}");
                }
            } => {}
        }
    }
}

#[tracing::instrument(level = "trace", skip_all)]
async fn connect_to_server_and_run_proxy(
    listener: &mut TcpListener,
    server_socket: TcpStream,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
) -> anyhow::Result<()> {
    let (server_read, server_write) = server_socket.into_split();
    let server_sender = launch_server_writer(server_write);

    let map = papaya::HashMap::default();
    let map: &'static papaya::HashMap<u64, PlayerHandle, FxBuildHasher> = Box::leak(Box::new(map));
    let egress = Egress::new(map);
    let egress = BufferedEgress::new(egress);

    let mut handler = IngressHandler::new(BufReader::new(server_read), egress);

    tokio::task::Builder::new()
        .name("s2prox")
        .spawn({
        let mut shutdown_rx = shutdown_rx.clone();

        async move {

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        println!("Received shutdown signal, exiting server reader loop");
                        return;
                    }
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

            shutdown_tx.send(true).unwrap();
        }
        .instrument(info_span!("server_reader_loop"))
    }).unwrap();

    let mut id_on = 0;

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

        let registry = map.pin();

        let (tx, rx) = kanal::bounded_async(1024);
        registry.insert(id_on, PlayerHandle {
            writer: tx,
            can_receive_broadcasts: AtomicBool::new(false),
        });

        // todo: some SlotMap like thing
        debug!("got player with id {id_on:?}");

        initiate_player_connection(
            socket,
            shutdown_rx.clone(),
            id_on,
            rx,
            server_sender.clone(),
            // player_registry.clone(),
        );

        id_on += 1;
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

        // println!("prox need to read {len} bytes");

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
