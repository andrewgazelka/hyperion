//! Player connection handling and packet processing.

use std::io::IoSlice;

use hyperion_proto::{PlayerConnect, PlayerDisconnect, PlayerPackets, ProxyToServerMessage};
use rkyv::ser::allocator::Arena;
use tokio::{
    io::{AsyncReadExt, AsyncWrite},
    task::JoinHandle,
};
use tracing::{debug, instrument, warn};

use crate::{
    cache::GlobalExclusionsManager, data::OrderedBytes, server_sender::ServerSender,
    util::AsyncWriteVectoredExt,
};

/// Default buffer size for reading player packets, set to 8 KiB.
const DEFAULT_READ_BUFFER_SIZE: usize = 8 * 1024;

/// Initiates a player connection handler, managing both incoming and outgoing packet streams.
///
/// This function sets up two asynchronous tasks:
/// 1. A reader task that processes incoming packets from the player.
/// 2. A writer task that sends outgoing packets to the player.
///
/// It also handles player disconnection and shutdown scenarios.
#[instrument(skip_all)]
pub fn initiate_player_connection(
    socket: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static,
    mut shutdown_signal: tokio::sync::watch::Receiver<bool>,
    player_id: u64,
    incoming_packet_receiver: kanal::AsyncReceiver<OrderedBytes>,
    server_sender: ServerSender,
) -> JoinHandle<()> {
    let (socket_reader, socket_writer) = tokio::io::split(socket);

    let mut socket_reader = Box::pin(socket_reader);
    let socket_writer = Box::pin(socket_writer);

    // Task for handling incoming packets (player -> proxy)
    let mut packet_reader_task = tokio::task::Builder::new()
        .name("PL->PR") // player to proxy
        .spawn(async move {
            let mut read_buffer = Vec::new();
            let player_stream_id = player_id;

            let connect = rkyv::to_bytes::<rkyv::rancor::Error>(
                &ProxyToServerMessage::PlayerConnect(PlayerConnect {
                    stream: player_stream_id,
                }),
            )
            .unwrap();

            server_sender.try_send(connect).unwrap();

            let mut arena = Arena::new();

            loop {
                // Ensure the buffer has enough capacity
                read_buffer.reserve(DEFAULT_READ_BUFFER_SIZE);

                let bytes_read = match socket_reader.read_buf(&mut read_buffer).await {
                    Ok(n) => n,
                    Err(e) => {
                        debug!("Error reading from player: {e:?}");
                        return server_sender;
                    }
                };

                if bytes_read == 0 {
                    debug!("End of stream reached for player");
                    return server_sender;
                }

                let player_packets = ProxyToServerMessage::PlayerPackets(PlayerPackets {
                    stream: player_id,
                    data: &read_buffer,
                });

                let aligned_vec = rkyv::api::high::to_bytes_with_alloc::<_, rkyv::rancor::Error>(
                    &player_packets,
                    arena.acquire(),
                )
                .unwrap();

                read_buffer.clear();

                if let Err(e) = server_sender.try_send(aligned_vec) {
                    debug!("Error forwarding player packets to server: {e:?}");
                    panic!("Error forwarding player packets to server: {e:?}");
                    // return server_sender;
                }
            }
        })
        .unwrap();

    // Task for handling outgoing packets (proxy -> player)
    let packet_writer_task = tokio::task::Builder::new()
        .name("proxy2player")
        .spawn(async move {
            let mut packet_writer = PlayerPacketWriter::new(socket_writer, player_id);

            while let Ok(outgoing_packet) = incoming_packet_receiver.recv().await {
                if outgoing_packet.is_flush() {
                    if let Err(e) = packet_writer.flush_pending_packets().await {
                        debug!("Error flushing packets to player: {e:?}");
                        return;
                    }
                } else {
                    packet_writer.enqueue_packet(outgoing_packet);
                }
            }
        })
        .unwrap();

    tokio::task::Builder::new()
        .name("player_disconnect")
        .spawn(async move {
            let shutdown_received = async move {
                shutdown_signal.wait_for(|value| *value).await.unwrap();
            };

            tokio::select! {
                () = shutdown_received => {
                    packet_reader_task.abort();
                    packet_writer_task.abort();
                }
                server_sender = &mut packet_reader_task => {
                    let Ok(server_sender) = server_sender else {
                        warn!("Player packet reader task failed unexpectedly");
                        return;
                    };
                    packet_writer_task.abort();

                    // Handle player disconnection
                    // player_registry.write().unwrap().remove(player_id);

                    debug!("Player disconnected: {player_id:?}");

                    let disconnect = rkyv::to_bytes::<rkyv::rancor::Error>(
                        &ProxyToServerMessage::PlayerDisconnect(PlayerDisconnect {
                            stream: player_id,
                        })).unwrap();

                    server_sender.send(disconnect).await.unwrap();
                }
            }
        })
        .unwrap()
}

/// Manages the writing of packets to a player's connection.
struct PlayerPacketWriter<W> {
    writer: W,
    player_id: u64,
    pending_packets: Vec<OrderedBytes>,
    io_vecs: Vec<IoSlice<'static>>,
}

impl<W: AsyncWrite + Unpin> PlayerPacketWriter<W> {
    /// Creates a new [`PlayerPacketWriter`] instance.
    const fn new(writer: W, player_id: u64) -> Self {
        Self {
            writer,
            player_id,
            pending_packets: Vec::new(),
            io_vecs: vec![],
        }
    }

    /// Adds a packet to the queue for writing.
    fn enqueue_packet(&mut self, packet: OrderedBytes) {
        self.pending_packets.push(packet);
    }

    /// Flushes all pending packets to the TCP writer.
    #[instrument(skip(self), fields(player_id = ?self.player_id), level = "trace")]
    async fn flush_pending_packets(&mut self) -> anyhow::Result<()> {
        for iovec in prepare_io_vectors(&mut self.pending_packets, self.player_id) {
            // extend lifetime of iovecs so we can reuse the io_vecs Vec
            let iovec = unsafe { std::mem::transmute::<IoSlice<'_>, IoSlice<'static>>(iovec) };
            self.io_vecs.push(iovec);
        }

        if self.io_vecs.is_empty() {
            self.pending_packets.clear();
            return Ok(());
        }

        self.writer.write_vectored_all(&mut self.io_vecs).await?;
        self.pending_packets.clear();
        self.io_vecs.clear();

        Ok(())
    }
}

/// Prepares IO vectors from the queue of ordered bytes, applying necessary exclusions.
fn prepare_io_vectors(
    packet_queue: &mut [OrderedBytes],
    player_id: u64,
) -> impl Iterator<Item = IoSlice<'_>> + '_ {
    packet_queue.sort_unstable_by_key(|packet| packet.order);

    packet_queue.iter_mut().flat_map(move |packet| {
        let packet_data = packet.data.as_ref();
        apply_exclusions(packet_data, packet.exclusions.as_deref(), player_id)
    })
}

/// Generates IO vectors to right given ranges of data that shuold be excluded.
fn apply_exclusions<'a>(
    packet_data: &'a [u8],
    exclusions: Option<&'a GlobalExclusionsManager>,
    player_id: u64,
) -> impl Iterator<Item = IoSlice<'a>> + 'a {
    let coroutine = #[coroutine]
    move || {
        let mut current_offset = 0;

        if let Some(exclusions) = exclusions {
            // Process exclusions in reverse order
            // We reverse the order because ExclusionIterator returns ranges starting from
            // the most recent (tail) node in the linked list structure (see @cache.rs).
            // By reversing, we process exclusions from the start of the packet to the end.
            let mut exclusion_ranges: heapless::Vec<_, 16> = heapless::Vec::new();

            for range in exclusions.exclusions_for_player(player_id) {
                exclusion_ranges.push(range).unwrap();
            }

            exclusion_ranges.reverse();

            // Iterate through the reversed ranges to properly exclude sections
            // from the beginning to the end of the packet
            for range in exclusion_ranges {
                let included_slice = &packet_data[current_offset..range.start];
                yield IoSlice::new(included_slice);
                current_offset = range.end;
            }
        }

        // Write remaining data or full packet if no exclusions
        if current_offset == 0 {
            yield IoSlice::new(packet_data);
        } else {
            let remaining_slice = &packet_data[current_offset..];
            yield IoSlice::new(remaining_slice);
        }
    };

    core::iter::from_coroutine(coroutine)
}
