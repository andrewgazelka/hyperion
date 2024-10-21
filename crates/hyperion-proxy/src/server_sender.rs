use std::io::IoSlice;

use hyperion_proto::{ArchivedProxyToServerMessage, ProxyToServerMessage};
use rkyv::util::AlignedVec;
use tokio::io::AsyncWriteExt;
use tracing::{trace_span, Instrument};

use crate::util::AsyncWriteVectoredExt;

const THRESHOLD_SEND: usize = 4 * 1024;

pub type ServerSender = tokio::sync::mpsc::Sender<AlignedVec>;

// todo: probably makes sense for caller to encode bytes
#[must_use]
pub fn launch_server_writer(mut write: tokio::net::tcp::OwnedWriteHalf) -> ServerSender {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AlignedVec>(65_536);

    tokio::task::Builder::new()
        .name("server_writer")
        .spawn(
            async move {
                let mut lengths: Vec<[u8; 8]> = Vec::new();
                let mut messages = Vec::new();

                while let Some(message) = rx.recv().await {
                    let len = message.len() as u64;

                    lengths.push(len.to_be_bytes());
                    messages.push(message);

                    while let Ok(message) = rx.try_recv() {
                        let len = message.len() as u64;
                        lengths.push(len.to_be_bytes());
                        messages.push(message);
                    }

                    // todo: remove allocation is there an easy way to do this?
                    let mut io_slices = Vec::new();

                    for (message, length) in messages.iter().zip(lengths.iter()) {
                        io_slices.push(IoSlice::new(length.as_ref()));
                        io_slices.push(IoSlice::new(message.as_ref()));
                    }

                    write.write_vectored_all(&mut io_slices).await.unwrap();
                    
                    lengths.clear();
                    messages.clear();
                }
            }
            .instrument(trace_span!("server_writer_loop")),
        )
        .unwrap();

    tx
}
