#![allow(unused)]
use anyhow::{bail, ensure};
use protocol_765::{serverbound, serverbound::NextState, status::Root};
use ser::{ExactPacket, ReadExtAsync, Readable, Writable, WritePacket};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
    time::sleep,
};
use tracing::{debug, error, info, instrument, warn};

struct Process {
    writer: WriteHalf<TcpStream>,
    reader: BufReader<ReadHalf<TcpStream>>,
}

impl Process {
    fn new(stream: TcpStream) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        let reader = BufReader::new(reader);
        // let writer = BufWriter::new(writer);
        Self { writer, reader }
    }

    #[instrument(skip(self))]
    async fn process(mut self, id: usize) -> anyhow::Result<()> {
        let bytes = self.reader.fill_buf().await?;

        if let Some(byte) = bytes.first() {
            if *byte == 0xfe {
                warn!("first byte: {:#x}", byte);
                self.status().await?;
                return Ok(());
            }
            warn!("no first byte");
        }

        let ExactPacket(serverbound::Handshake {
            protocol_version,
            server_address,
            server_port,
            next_state,
        }) = self.reader.read_type().await?;

        ensure!(protocol_version.0 == 765, "expected protocol version 765");
        ensure!(server_port == 25565, "expected server port 25565");

        match next_state {
            NextState::Status => self.status().await?,
            NextState::Login => self.login().await?,
        }

        Ok(())
    }

    async fn login(mut self) -> anyhow::Result<()> {
        info!("login");

        let ExactPacket(serverbound::LoginStart { username, uuid }) =
            self.reader.read_type().await?;

        debug!("username: {username}");
        debug!("uuid: {uuid}");

        Ok(())
    }

    async fn status(mut self) -> anyhow::Result<()> {
        info!("status");
        let ExactPacket(serverbound::StatusRequest) = self.reader.read_type().await?;

        info!("byte");

        let mut json = json!({
            "version": {
                "name": "1.20.4",
                "protocol": 765,
            },
            "players": {
                "online": 0,
                "max": 10_000,
                "sample": [],
            },
            "description": "10k babyyyyy",
        });

        let send = WritePacket::new(protocol_765::clientbound::StatusResponse {
            json: json.to_string(),
        });

        send.write_async(&mut self.writer).await?;

        info!("wrote status response");

        let ExactPacket(serverbound::Ping { payload }) = self.reader.read_type().await?;

        info!("read ping {}", payload);

        let pong = WritePacket::new(protocol_765::clientbound::Pong { payload });
        pong.write_async(&mut self.writer).await?;

        Ok(())
    }
}

async fn print_errors(future: impl std::future::Future<Output = anyhow::Result<()>>) {
    if let Err(err) = future.await {
        error!("{:?}", err);
    }
}

// #[allow(clippy::infinite_loop)]
// async fn process(id: usize, stream: TcpStream) -> anyhow::Result<()> {
//     println!("{handshake:?}");
//
//     ensure!(
//         handshake.data.next_state == NextState::Status,
//         "expected status"
//     );
//
//     let _status_pkt = ExactPacket::<serverbound::StatusRequest>::read_async(&mut reader).await?;
//     // ensure!(status_pkt.0 == 0, "expected status packet");
//
//     let mut writer = BufWriter::new(writer);
//
//     let json = Root::sample();
//     let json = serde_json::to_string_pretty(&json)?;
//
//     println!("{}", json);
//
//     let response = protocol_765::clientbound::StatusResponse { json };
//
//     let login_start = WritePacket::new(response);
//     login_start.write_async(&mut writer).await?;
//
//     debug!("wrote status response");
//
//     let pong = WritePacket::new(protocol_765::clientbound::Pong { payload: 0 });
//     pong.write_async(&mut writer).await?;
//
//     println!("wrote pong");
//
//     // wait for the client to disconnect
//     // loop {
//     //     writer.write_u8(0).await?;
//     //     // let byte = reader.read_u8().await?;
//     //     sleep(core::time::Duration::from_millis(10)).await;
//     // }
//
//     while let Ok(byte) = reader.read_u8().await {
//         println!("{:#x}", byte);
//     }
//     // let x = reader.read_u8().await?;
//     //
//     // println!("{:#x}", x);
//
//     println!("done");
//
//     Ok(())
// }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    // start socket 25565
    let listener = TcpListener::bind("0.0.0.0:25565").await?;

    let mut id = 0;

    // accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        let process = Process::new(stream);
        let action = process.process(id);
        let action = print_errors(action);

        tokio::spawn(action);
        id += 1;
    }
}
