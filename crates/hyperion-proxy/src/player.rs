use std::{cmp::Reverse, collections::BinaryHeap, io::IoSlice, sync::Arc};

use bytes::Bytes;
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
                writer.queue(incoming);

                while let Ok(data) = proxy_to_player_rx.try_recv() {
                    writer.queue(data);
                }

                if let Err(e) = writer.flush_tcp_writer().await {
                    debug!("error flushing to player: {e:?}");
                    return;
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
    order_on: u64,
    id: PlayerId,
    queue: BinaryHeap<Reverse<OrderedBytes>>,
    to_write: Vec<Bytes>,
}

impl PlayerWriter {
    fn new(writer: tokio::net::tcp::OwnedWriteHalf, id: PlayerId) -> Self {
        Self {
            writer,
            order_on: 0,
            queue: BinaryHeap::default(),
            id,
            to_write: vec![],
        }
    }

    fn queue(&mut self, incoming: OrderedBytes) {
        if incoming.order > self.order_on {
            // we need to wait because this packet was sent out of order;
            // we must wait until we get all the packets leading up to this one

            self.queue.push(Reverse(incoming));
            return;
        }

        // we can immediately write
        self.write_all(incoming.data, incoming.exclusions.as_deref());

        self.order_on = (incoming.order + 1).max(self.order_on);

        // let's see if we can write anything else
        self.flush_queue();
    }

    fn flush_queue(&mut self) {
        while let Some(Reverse(peek)) = self.queue.peek() {
            if peek.order > self.order_on {
                break;
            }

            self.order_on = (peek.order + 1).max(self.order_on);
            let data = self.queue.pop().unwrap().0;

            self.write_all(data.data, data.exclusions.as_deref());
        }
    }

    fn write_all(&mut self, data: Bytes, exclusions: Option<&GlobalExclusions>) {
        let mut on = 0;

        if let Some(exclusions) = exclusions {
            // get exclusions and make order correct
            // todo: does this panic if over 16?
            let mut exclusions_vec: heapless::Vec<_, 16> = heapless::Vec::new();

            for elem in exclusions.exclusions_for_player(self.id) {
                exclusions_vec.push(elem).unwrap();
            }

            exclusions_vec.reverse();

            for range in exclusions_vec {
                self.to_write.push(data.slice(on..range.start));
                on = range.end;
            }
        };

        if on == 0 {
            self.to_write.push(data);
        } else {
            self.to_write.push(data.slice(on..));
        }
    }

    async fn flush_tcp_writer(&mut self) -> anyhow::Result<()> {
        {
            if self.to_write.is_empty() {
                return Ok(());
            }

            let mut vec = Vec::new();
            vec.extend(self.to_write.iter().map(|data| IoSlice::new(data)));

            let mut iovecs = &mut vec[..];

            loop {
                let n = self.writer.write_vectored(iovecs).await?;

                IoSlice::advance_slices(&mut iovecs, n);

                if iovecs.is_empty() {
                    break;
                }
            }
        }

        self.to_write.clear();

        Ok(())
    }
}
