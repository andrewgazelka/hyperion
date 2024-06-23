use hyperion_proto::{ProxyToServer, ProxyToServerMessage};
use prost::Message;
use tokio::io::AsyncWriteExt;
use tracing::{trace_span, Instrument};

const THRESHOLD_SEND: usize = 4 * 1024;

pub type ServerSender = tokio::sync::mpsc::Sender<ProxyToServerMessage>;

// todo: probably makes sense for caller to encode bytes
#[must_use]
pub fn launch_server_writer(mut write: tokio::net::tcp::OwnedWriteHalf) -> ServerSender {
    let (tx, mut rx) = tokio::sync::mpsc::channel(65_536);

    tokio::spawn(
        async move {
            let mut bytes = Vec::with_capacity(8 * 1024);
            while let Some(message) = rx.recv().await {
                write_message(&mut bytes, message);

                loop {
                    if bytes.len() >= THRESHOLD_SEND {
                        break;
                    }

                    let Ok(message) = rx.try_recv() else {
                        break;
                    };

                    write_message(&mut bytes, message);
                }

                write.write_all(&bytes).await.unwrap();
                bytes.clear();
            }
        }
        .instrument(trace_span!("server_writer_loop")),
    );

    tx
}

fn write_message(write: &mut Vec<u8>, message: ProxyToServerMessage) {
    let message = ProxyToServer {
        proxy_to_server_message: Some(message),
    };

    message.encode_length_delimited(write).unwrap();
}
