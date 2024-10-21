use std::io::IoSlice;

use rkyv::util::AlignedVec;
use tracing::{trace_span, Instrument};

use crate::util::AsyncWriteVectoredExt;

pub type ServerSender = kanal::AsyncSender<AlignedVec>;

// todo: probably makes sense for caller to encode bytes
#[must_use]
pub fn launch_server_writer(mut write: tokio::net::tcp::OwnedWriteHalf) -> ServerSender {
    let (tx, rx) = kanal::bounded_async::<AlignedVec>(32_768);

    tokio::task::Builder::new()
        .name("server_writer")
        .spawn(
            async move {
                let mut lengths: Vec<[u8; 8]> = Vec::new();
                let mut messages = Vec::new();

                // todo: remove allocation is there an easy way to do this?
                let mut io_slices = Vec::new();

                while let Ok(message) = rx.recv().await {
                    let len = message.len() as u64;

                    lengths.push(len.to_be_bytes());
                    messages.push(message);

                    while let Ok(Some(message)) = rx.try_recv() {
                        let len = message.len() as u64;
                        lengths.push(len.to_be_bytes());
                        messages.push(message);
                    }

                    for (message, length) in messages.iter().zip(lengths.iter()) {
                        let len = IoSlice::new(length);
                        let msg = IoSlice::new(message);

                        // todo: is there a way around this?
                        let len =
                            unsafe { core::mem::transmute::<IoSlice<'_>, IoSlice<'static>>(len) };
                        let msg =
                            unsafe { core::mem::transmute::<IoSlice<'_>, IoSlice<'static>>(msg) };

                        io_slices.push(len);
                        io_slices.push(msg);
                    }

                    write.write_vectored_all(&mut io_slices).await.unwrap();

                    lengths.clear();
                    messages.clear();
                    io_slices.clear();
                }
            }
            .instrument(trace_span!("server_writer_loop")),
        )
        .unwrap();

    tx
}
