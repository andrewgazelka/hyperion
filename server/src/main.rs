#![allow(unused)]

use std::{io, io::ErrorKind};

use anyhow::{bail, ensure, Context};
use bytes::{Buf, BufMut, BytesMut};
use protocol_765::{clientbound, serverbound, serverbound::NextState, status::Root};
use ser::{
    types::{VarInt, VarIntDecodeError, MAX_PACKET_SIZE},
    ExactPacket, Packet, Readable, Writable, WritePacket,
};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
    time::sleep,
};
use tracing::{debug, error, info, instrument, warn};

#[derive(Default)]
struct PacketDecoder {
    buf: BytesMut,
}

#[derive(Clone, Debug, Default)]
pub struct PacketFrame {
    /// The ID of the decoded packet.
    pub id: i32,
    /// The contents of the packet after the leading VarInt ID.
    pub body: BytesMut,
}

impl PacketFrame {
    pub fn decode<'a, P>(&'a self) -> anyhow::Result<P>
    where
        P: Packet + Readable<'a>,
    {
        ensure!(
            P::ID == self.id,
            "packet ID mismatch while decoding '{}': expected {}, got {}",
            P::NAME,
            P::ID,
            self.id
        );

        #[allow(clippy::min_ident_chars)]
        let mut r = &*self.body;

        let pkt = P::decode(&mut r)?;

        ensure!(
            r.is_empty(),
            "missed {} bytes while decoding '{}'",
            r.len(),
            P::NAME
        );

        Ok(pkt)
    }
}

impl PacketDecoder {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::min_ident_chars
    )]
    pub fn try_next_packet(&mut self) -> anyhow::Result<Option<PacketFrame>> {
        let mut r = &*self.buf;

        let packet_len = match VarInt::decode_partial(&mut r) {
            Ok(len) => len,
            Err(VarIntDecodeError::Incomplete) => return Ok(None),
            Err(VarIntDecodeError::TooLarge) => bail!("malformed packet length VarInt"),
        };

        ensure!(
            (0..=MAX_PACKET_SIZE).contains(&packet_len),
            "packet length of {packet_len} is out of bounds"
        );

        if r.len() < packet_len as usize {
            // Not enough data arrived yet.
            return Ok(None);
        }

        let packet_len_len = VarInt(packet_len).written_size();

        let mut data;

        self.buf.advance(packet_len_len);

        data = self.buf.split_to(packet_len as usize);

        // Decode the leading packet ID.
        r = &*data;
        let packet_id = VarInt::decode(&mut r)
            .context("failed to decode packet ID")?
            .0;

        data.advance(data.len() - r.len());

        Ok(Some(PacketFrame {
            id: packet_id,
            body: data,
        }))
    }

    pub fn queue_bytes(&mut self, mut bytes: BytesMut) {
        self.buf.unsplit(bytes);
    }

    pub fn take_capacity(&mut self) -> BytesMut {
        self.buf.split_off(self.buf.len())
    }

    pub fn reserve(&mut self, additional: usize) {
        self.buf.reserve(additional);
    }
}

const READ_BUF_SIZE: usize = 4096;

#[derive(Default)]
pub struct PacketEncoder {
    buf: BytesMut,
}

impl PacketEncoder {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::min_ident_chars
    )]
    pub fn append_packet<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: Packet + Writable,
    {
        let start_len = self.buf.len();

        let mut writer = (&mut self.buf).writer();
        VarInt(P::ID).write(&mut writer)?;

        pkt.write(&mut writer)?;

        let data_len = self.buf.len() - start_len;

        let packet_len = data_len;

        ensure!(
            packet_len <= MAX_PACKET_SIZE as usize,
            "packet exceeds maximum length"
        );

        let packet_len_size = VarInt(packet_len as i32).written_size();

        self.buf.put_bytes(0, packet_len_size);
        self.buf
            .copy_within(start_len..start_len + data_len, start_len + packet_len_size);

        #[allow(clippy::indexing_slicing)]
        let mut front = &mut self.buf[start_len..];
        VarInt(packet_len as i32).write(&mut front)?;

        Ok(())
    }

    /// Takes all the packets written so far and encrypts them if encryption is
    /// enabled.
    pub fn take(&mut self) -> BytesMut {
        self.buf.split()
    }
}

struct Io {
    stream: TcpStream,
    dec: PacketDecoder,
    enc: PacketEncoder,
    frame: PacketFrame,
}

impl Io {
    pub async fn recv_packet<'a, P>(&'a mut self) -> anyhow::Result<P>
    where
        P: Packet + Readable<'a>,
    {
        loop {
            if let Some(frame) = self.dec.try_next_packet()? {
                self.frame = frame;
                return self.frame.decode();
            }

            self.dec.reserve(READ_BUF_SIZE);
            let mut buf = self.dec.take_capacity();

            if self.stream.read_buf(&mut buf).await? == 0 {
                return Err(io::Error::from(ErrorKind::UnexpectedEof).into());
            }

            // This should always be an O(1) unsplit because we reserved space earlier and
            // the call to `read_buf` shouldn't have grown the allocation.
            self.dec.queue_bytes(buf);
        }
    }

    pub async fn recv_packet_raw(&mut self) -> anyhow::Result<PacketFrame> {
        loop {
            if let Some(frame) = self.dec.try_next_packet()? {
                return Ok(frame);
            }

            self.dec.reserve(READ_BUF_SIZE);
            let mut buf = self.dec.take_capacity();

            if self.stream.read_buf(&mut buf).await? == 0 {
                return Err(io::Error::from(ErrorKind::UnexpectedEof).into());
            }

            // This should always be an O(1) unsplit because we reserved space earlier and
            // the call to `read_buf` shouldn't have grown the allocation.
            self.dec.queue_bytes(buf);
        }
    }

    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            dec: PacketDecoder::default(),
            enc: PacketEncoder::default(),
            frame: PacketFrame::default(),
        }
    }

    pub(crate) async fn send_packet<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: Packet + Writable,
    {
        self.enc.append_packet(pkt)?;
        let bytes = self.enc.take();
        self.stream.write_all(&bytes).await?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn process(mut self, id: usize) -> anyhow::Result<()> {
        let serverbound::Handshake {
            protocol_version,
            server_address,
            server_port,
            next_state,
        } = self.recv_packet().await?;

        ensure!(protocol_version.0 == 765, "expected protocol version 765");
        ensure!(server_port == 25565, "expected server port 25565");

        match next_state {
            NextState::Status => self.status().await?,
            NextState::Login => self.login().await?,
        }

        Ok(())
    }

    // The login process is as follows:
    // 1. C→S: Handshake with Next State set to 2 (login)
    // 2. C→S: Login Start
    // 3. S→C: Encryption Request
    // 4. Client auth
    // 5. C→S: Encryption Response
    // 6. Server auth, both enable encryption
    // 7. S→C: Set Compression (optional)
    // 8. S→C: Login Success
    // 9. C→S: Login Acknowledged
    async fn login(mut self) -> anyhow::Result<()> {
        debug!("login");

        let serverbound::LoginStart { username, uuid } = self.recv_packet().await?;

        debug!("username: {username}");
        debug!("uuid: {uuid}");

        let username = username.to_owned();

        let packet = clientbound::LoginSuccess {
            uuid,
            username: &username,
            properties: vec![],
        };

        debug!("sending {packet:?}");

        self.send_packet(&packet).await?;

        let serverbound::LoginAcknowledged = self.recv_packet().await?;

        debug!("received login acknowledged");

        self.main_loop().await?;

        Ok(())
    }

    async fn main_loop(mut self) -> anyhow::Result<()> {
        loop {
            let packet = self.recv_packet_raw().await?;
            debug!("received {packet:?}");
        }
    }

    async fn status(mut self) -> anyhow::Result<()> {
        debug!("status");
        let serverbound::StatusRequest = self.recv_packet().await?;

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

        let json = serde_json::to_string_pretty(&json)?;

        let send = clientbound::StatusResponse { json: &json };

        self.send_packet(&send).await?;

        debug!("wrote status response");

        let serverbound::Ping { payload } = self.recv_packet().await?;

        debug!("read ping {}", payload);

        let pong = clientbound::Pong { payload };
        self.send_packet(&pong).await?;

        Ok(())
    }
}

async fn print_errors(future: impl core::future::Future<Output = anyhow::Result<()>>) {
    if let Err(err) = future.await {
        error!("{:?}", err);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    // start socket 25565
    let listener = TcpListener::bind("0.0.0.0:25565").await?;

    let mut id = 0;

    // accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        let process = Io::new(stream);
        let action = process.process(id);
        let action = print_errors(action);

        tokio::spawn(action);
        id += 1;
    }
}
