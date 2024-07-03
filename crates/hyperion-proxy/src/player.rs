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
    cache::GlobalExclusions,
    data::{OrderedBytes, PlayerId, PlayerRegistry},
    server_sender::ServerSender,
};

const DEFAULT_BUFFER_SIZE: usize = 8 * 1024;

#[instrument(skip_all)]
pub fn launch_player(
    socket: TcpStream,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    stream: PlayerId,
    mut proxy_to_player_rx: tokio::sync::mpsc::Receiver<OrderedBytes>,
    server_sender: ServerSender,
    registry: Arc<PlayerRegistry>,
) -> JoinHandle<()> {
    let (mut read, write) = socket.into_split();

    // reading ::: player -> proxy
    let mut player_writer = tokio::spawn(
        async move {
            let mut buffer = bytes::BytesMut::with_capacity(DEFAULT_BUFFER_SIZE);
            let stream = stream.data().as_ffi();

            server_sender
                .send(ProxyToServerMessage::PlayerConnect(PlayerConnect {
                    stream,
                }))
                .await
                .unwrap();

            loop {
                // Ensure the buffer has enough capacity
                buffer.reserve(DEFAULT_BUFFER_SIZE);

                let n = match read.read_buf(&mut buffer).await {
                    Ok(n) => n,
                    Err(e) => {
                        debug!("error reading from player: {e:?}");
                        return server_sender;
                    }
                };

                if n == 0 {
                    debug!("EOF reached from player");
                    return server_sender;
                }
                // Split the buffer and send the data
                let data = buffer.split().freeze();

                let msg = ProxyToServerMessage::PlayerPackets(PlayerPackets { data, stream });

                if let Err(e) = server_sender.send(msg).await {
                    debug!("error sending to server: {e:?}");
                    return server_sender;
                }
            }
        }
        .instrument(trace_span!("player_read_loop")),
    );

    // writing ::: proxy -> player
    let player_reader = tokio::spawn(
        async move {
            let mut writer = PlayerWriter::new(write, stream);

            while let Some(incoming) = proxy_to_player_rx.recv().await {
                if incoming.is_flush() {
                    if let Err(e) = writer.flush_tcp_writer().await {
                        debug!("error flushing to player: {e:?}");
                        return;
                    }
                } else {
                    writer.queue(incoming);
                }
            }
        }
        .instrument(trace_span!("player_write_loop")),
    );

    tokio::spawn(async move {
        let is_shutdown = async move {
            shutdown.wait_for(|value| *value).await.unwrap();
        };

        tokio::select! {
        () = is_shutdown => {
            player_writer.abort();
            player_reader.abort();
        }
        sender = &mut player_writer => {
            let Ok(sender) = sender else {
                warn!("player_writer failed");
                return;
            };
            player_reader.abort();

            // on disconnect/error
            registry.write().unwrap().remove(stream);

            info!("Disconnected player with id {stream:?}");

            let stream = stream.data().as_ffi();


            sender
                .send(ProxyToServerMessage::PlayerDisconnect(PlayerDisconnect {
                    stream,
                }))
                .await
                .unwrap();
                }
            }
    })
}

struct PlayerWriter {
    writer: tokio::net::tcp::OwnedWriteHalf,
    id: PlayerId,
    queue: Vec<OrderedBytes>,
}

impl PlayerWriter {
    fn new(writer: tokio::net::tcp::OwnedWriteHalf, id: PlayerId) -> Self {
        Self {
            writer,
            queue: Vec::default(),
            id,
        }
    }

    fn queue(&mut self, incoming: OrderedBytes) {
        self.queue.push(incoming);
    }

    // intstrument id

    #[instrument(skip(self), fields(player_id = ?self.id))]
    async fn flush_tcp_writer(&mut self) -> anyhow::Result<()> {
        // todo: we should not need to re-allocate every single time we flush
        let mut vec = Vec::new();

        flush_queue(&mut self.queue, self.id, &mut vec);

        if vec.is_empty() {
            // probably do not need extra clear but have not checked edge cases
            self.queue.clear();
            return Ok(());
        }

        let mut iovecs = &mut vec[..];

        loop {
            let n = self.writer.write_vectored(iovecs).await?;

            IoSlice::advance_slices(&mut iovecs, n);

            if iovecs.is_empty() {
                break;
            }
        }

        self.queue.clear();

        Ok(())
    }
}

fn flush_queue<'a>(queue: &'a mut Vec<OrderedBytes>, id: PlayerId, io_vecs: &mut Vec<IoSlice<'a>>) {
    queue.sort_unstable_by_key(|x| x.order);

    for data in queue {
        let slice = data.data.as_ref();
        write_all(slice, data.exclusions.as_deref(), id, io_vecs);
    }
}

fn write_all<'a>(
    data: &'a [u8],
    exclusions: Option<&GlobalExclusions>,
    id: PlayerId,
    to_write: &mut Vec<IoSlice<'a>>,
) {
    let mut on = 0;

    if let Some(exclusions) = exclusions {
        // get exclusions and make order correct
        // todo: does this panic if over 16?
        let mut exclusions_vec: heapless::Vec<_, 16> = heapless::Vec::new();

        for elem in exclusions.exclusions_for_player(id) {
            exclusions_vec.push(elem).unwrap();
        }

        exclusions_vec.reverse();

        for range in exclusions_vec {
            let slice = &data[on..range.start];
            let slice = IoSlice::new(slice);
            to_write.push(slice);
            on = range.end;
        }
    };

    if on == 0 {
        to_write.push(IoSlice::new(data));
    } else {
        let slice = &data[on..];
        let slice = IoSlice::new(slice);
        to_write.push(slice);
    }
}
