//! Player connection handling and packet processing.

use std::{io::IoSlice, sync::Arc};

use hyperion_proto::{PlayerConnect, PlayerDisconnect, PlayerPackets, ProxyToServerMessage};
use prost::bytes;
use slotmap::Key;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    task::JoinHandle,
};
use tracing::{debug, info, instrument, trace_span, warn, Instrument};

use crate::{
    cache::ExclusionManager,
    data::{OrderedBytes, PlayerId, PlayerRegistry},
    server_sender::ServerSender,
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
    socket: TcpStream,
    mut shutdown_signal: tokio::sync::watch::Receiver<bool>,
    player_id: PlayerId,
    mut incoming_packet_receiver: tokio::sync::mpsc::Receiver<OrderedBytes>,
    server_sender: ServerSender,
    player_registry: Arc<PlayerRegistry>,
) -> JoinHandle<()> {
    let (mut socket_reader, socket_writer) = socket.into_split();

    // Task for handling incoming packets (player -> proxy)
    let mut packet_reader_task = tokio::spawn(
        async move {
            let mut read_buffer = bytes::BytesMut::with_capacity(DEFAULT_READ_BUFFER_SIZE);
            let player_stream_id = player_id.data().as_ffi();

            server_sender
                .send(ProxyToServerMessage::PlayerConnect(PlayerConnect {
                    stream: player_stream_id,
                }))
                .await
                .unwrap();

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

                // Process and forward the received data
                let packet_data = read_buffer.split().freeze();
                let player_packets = PlayerPackets {
                    data: packet_data,
                    stream: player_stream_id,
                };

                if let Err(e) = server_sender
                    .send(ProxyToServerMessage::PlayerPackets(player_packets))
                    .await
                {
                    debug!("Error forwarding player packets to server: {e:?}");
                    return server_sender;
                }
            }
        }
        .instrument(trace_span!("player_packet_reader")),
    );

    // Task for handling outgoing packets (proxy -> player)
    let packet_writer_task = tokio::spawn(
        async move {
            let mut packet_writer = PlayerPacketWriter::new(socket_writer, player_id);

            while let Some(outgoing_packet) = incoming_packet_receiver.recv().await {
                if outgoing_packet.is_flush() {
                    if let Err(e) = packet_writer.flush_pending_packets().await {
                        debug!("Error flushing packets to player: {e:?}");
                        return;
                    }
                } else {
                    packet_writer.enqueue_packet(outgoing_packet);
                }
            }
        }
        .instrument(trace_span!("player_packet_writer")),
    );

    tokio::spawn(async move {
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
                player_registry.write().unwrap().remove(player_id);

                info!("Player disconnected: {player_id:?}");

                let player_stream_id = player_id.data().as_ffi();

                server_sender
                    .send(ProxyToServerMessage::PlayerDisconnect(PlayerDisconnect {
                        stream: player_stream_id,
                    }))
                    .await
                    .unwrap();
            }
        }
    })
}

/// Manages the writing of packets to a player's connection.
struct PlayerPacketWriter {
    tcp_writer: tokio::net::tcp::OwnedWriteHalf,
    player_id: PlayerId,
    pending_packets: Vec<OrderedBytes>,
}

impl PlayerPacketWriter {
    /// Creates a new PlayerPacketWriter instance.
    const fn new(tcp_writer: tokio::net::tcp::OwnedWriteHalf, player_id: PlayerId) -> Self {
        Self {
            tcp_writer,
            player_id,
            pending_packets: Vec::new(),
        }
    }

    /// Adds a packet to the queue for writing.
    fn enqueue_packet(&mut self, packet: OrderedBytes) {
        self.pending_packets.push(packet);
    }

    /// Flushes all pending packets to the TCP writer.
    #[instrument(skip(self), fields(player_id = ?self.player_id))]
    async fn flush_pending_packets(&mut self) -> anyhow::Result<()> {
        let mut io_vectors = Vec::new();

        for iovec in prepare_io_vectors(&mut self.pending_packets, self.player_id) {
            io_vectors.push(iovec);
        }

        if io_vectors.is_empty() {
            self.pending_packets.clear();
            return Ok(());
        }

        self.tcp_writer.write_vectored_all(&mut io_vectors).await?;
        self.pending_packets.clear();

        Ok(())
    }
}

/// Prepares IO vectors from the queue of ordered bytes, applying necessary exclusions.
fn prepare_io_vectors(
    packet_queue: &mut [OrderedBytes],
    player_id: PlayerId,
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
    exclusions: Option<&'a ExclusionManager>,
    player_id: PlayerId,
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
