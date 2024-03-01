#![allow(unused_imports)]

use protocol_765::{clientbound, serverbound, serverbound::StatusRequest};
use ser::{ExactPacket, ReadExtAsync, Writable, WritePacket};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    // connect to localhost:25565
    let stream = TcpStream::connect("localhost:25565").await?;

    let (reader, mut writer) = tokio::io::split(stream);

    let mut reader = tokio::io::BufReader::new(reader);

    let handshake = serverbound::Handshake {
        protocol_version: 765.into(),
        server_address: "localhost".to_owned(),
        server_port: 25565,
        next_state: serverbound::NextState::Status,
    };

    WritePacket::new(handshake).write_async(&mut writer).await?;

    // writer.flush().await?;

    println!("wrote handshake");

    WritePacket::new(StatusRequest)
        .write_async(&mut writer)
        .await?;

    println!("wrote status request");

    let ExactPacket(clientbound::StatusResponse { json }) = reader.read_type().await?;

    println!("read status response json: {}", json);

    Ok(())
}
