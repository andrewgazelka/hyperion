#![expect(clippy::future_not_send, reason = "monoio is not Send")]

use std::{
    borrow::Cow,
    io,
    io::ErrorKind,
    net::{ToSocketAddrs},
    os::fd::{AsRawFd, RawFd},
    ptr::addr_of_mut,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{ensure, Context};
use base64::Engine;
use bytes::BytesMut;
use monoio::{
    io::{AsyncReadRent, AsyncWriteRentExt, OwnedReadHalf, OwnedWriteHalf, Splitable},
    net::{TcpListener, TcpStream},
};
use serde_json::json;
use sha2::Digest;
use tracing::{debug, error, instrument, trace, warn};
use valence_protocol::{
    decode::PacketFrame,
    nbt::{compound, Compound, List},
    packets::{
        handshaking::{handshake_c2s::HandshakeNextState, HandshakeC2s},
        login::{LoginHelloC2s, LoginSuccessS2c},
        status,
    },
    uuid::Uuid,
    Bounded, Decode, Encode, PacketDecoder, PacketEncoder, VarInt,
};
use valence_registry::RegistryCodec;

use crate::{config, SHARED};

const DEFAULT_SPEED: u32 = 1024 * 1024;

const READ_BUF_SIZE: usize = 4096;

/// The Minecraft protocol version this library currently targets.
pub const PROTOCOL_VERSION: i32 = 763;

pub const MAX_JAVA_PACKET_SIZE: usize = 0x001F_FFFF;
// pub const MAX_BEDROCK_PACKET_SIZE: usize = 0x0010_0000;

// max
pub const MAX_PACKET_SIZE: usize = MAX_JAVA_PACKET_SIZE;

/// The stringified name of the Minecraft version this library currently
/// targets.
pub const MINECRAFT_VERSION: &str = "1.20.1";

fn offline_uuid(username: &str) -> anyhow::Result<Uuid> {
    let digest = sha2::Sha256::digest(username);

    #[expect(clippy::indexing_slicing, reason = "sha256 is always 32 bytes")]
    let slice = &digest[..16];

    Uuid::from_slice(slice).context("failed to create uuid")
}

pub struct ClientConnection {
    pub packets: Packets,
    pub name: Box<str>,
    pub uuid: Uuid,
}

pub struct Io {
    stream: TcpStream,
    dec: PacketDecoder,
    enc: PacketEncoder,
    frame: PacketFrame,
}

pub struct IoWrite {
    write: OwnedWriteHalf<TcpStream>,
    raw_fd: RawFd,
}

pub struct IoRead {
    stream: OwnedReadHalf<TcpStream>,
    dec: PacketDecoder,
}

pub struct WriterComm {
    tx: flume::Sender<bytes::Bytes>,
    enc: PacketEncoder,

    /// Approximate speed that the other side can receive the data that this sends.
    /// Measured in bytes/second.
    speed_mib_per_second: Arc<AtomicU32>,
}

impl WriterComm {
    pub fn is_closed(&self) -> bool {
        self.tx.is_disconnected()
    }

    #[expect(
        dead_code,
        reason = "not used, but we plan it to be used in the future"
    )]
    pub fn speed_mib_per_second(&self) -> u32 {
        self.speed_mib_per_second.load(Ordering::Relaxed)
    }

    pub fn serialize<P>(&mut self, pkt: &P) -> anyhow::Result<bytes::Bytes>
    where
        P: valence_protocol::Packet + Encode,
    {
        self.enc.append_packet(pkt)?;
        let bytes = self.enc.take();

        Ok(bytes.freeze())
    }

    pub fn send_raw(&self, bytes: bytes::Bytes) -> anyhow::Result<()> {
        trace!("send raw bytes");
        self.tx.send(bytes)?;
        Ok(())
    }

    pub(crate) fn send_packet<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        let bytes = self.serialize(pkt)?;
        trace!("send packet {}", P::NAME);
        self.send_raw(bytes)?;
        Ok(())
    }
}

pub struct UserPacketFrame {
    pub packet: PacketFrame,
    pub user: Uuid,
}

impl IoRead {
    pub async fn recv_packet_raw(&mut self) -> anyhow::Result<PacketFrame> {
        loop {
            if let Some(frame) = self.dec.try_next_packet()? {
                return Ok(frame);
            }

            self.dec.reserve(READ_BUF_SIZE);
            let buf = self.dec.take_capacity();

            let (bytes_read, buf) = self.stream.read(buf).await;

            let bytes_read = bytes_read?;

            if bytes_read == 0 {
                return Err(io::Error::from(ErrorKind::UnexpectedEof).into());
            }

            // This should always be an O(1) unsplit because we reserved space earlier and
            // the call to `read_buf` shouldn't have grown the allocation.
            self.dec.queue_bytes(buf);
        }
    }
}

impl IoWrite {
    pub(crate) async fn send_packet(&mut self, bytes: bytes::Bytes) -> io::Result<()> {
        // self.write.write_k

        let (result, _) = self.write.write_all(bytes).await;

        result?; // error occurs here

        Ok(())
    }

    /// This function returns the number of bytes in the TCP send queue that have
    /// been sent but have not been acknowledged by the client.
    ///
    /// If running on non-Unix systems, it currently returns `0` by default.
    ///
    /// Proper error handling for `ioctl` failures should be added, and support for other operating
    /// systems needs to be considered for portability.
    pub(crate) fn queued_send(&self) -> libc::c_int {
        #[cfg(target_os = "linux")]
        {
            let mut value: libc::c_int = 0;
            // SAFETY: raw_fd is valid since the TcpStream is still alive, and value is valid to
            // write to
            unsafe {
                // TODO: Handle ioctl error properly
                assert_ne!(
                    libc::ioctl(self.raw_fd, libc::TIOCOUTQ, addr_of_mut!(value)),
                    -1
                );
            }
            value
        }

        #[cfg(target_os = "macos")]
        {
            let mut value: libc::c_int = 0;
            let mut len: libc::socklen_t =
                libc::socklen_t::try_from(std::mem::size_of::<libc::c_int>()).unwrap();
            // SAFETY: raw_fd is valid since the TcpStream is still alive, value and len are valid
            // to write to, and value and len do not alias
            unsafe {
                // TODO: Handle getsockopt error properly
                assert_ne!(
                    libc::getsockopt(
                        self.raw_fd,
                        libc::SOL_SOCKET,
                        libc::SO_NWRITE,
                        addr_of_mut!(value).cast(),
                        addr_of_mut!(len)
                    ),
                    -1
                );
            }
            value
        }

        // TODO: Support getting queued send for other OS
    }
}

pub struct Packets {
    pub writer: WriterComm,
}

impl Io {
    pub async fn recv_packet<'a, P>(&'a mut self) -> anyhow::Result<P>
    where
        P: valence_protocol::Packet + Decode<'a>,
    {
        loop {
            if let Some(frame) = self.dec.try_next_packet()? {
                self.frame = frame;
                let decode: P = self.frame.decode()?;
                return Ok(decode);
            }

            self.dec.reserve(READ_BUF_SIZE);
            let buf = self.dec.take_capacity();

            if buf.len() > MAX_PACKET_SIZE {
                return Err(io::Error::from(ErrorKind::InvalidData).into());
            }

            let (bytes_read, buf) = self.stream.read(buf).await;
            let bytes_read = bytes_read?;

            if bytes_read == 0 {
                return Err(io::Error::from(ErrorKind::UnexpectedEof).into());
            }

            debug!("read {bytes_read} bytes");

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
            frame: PacketFrame {
                id: 0,
                body: BytesMut::new(),
            },
        }
    }

    pub(crate) async fn send_packet<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        self.enc.append_packet(pkt)?;
        let bytes = self.enc.take();

        let mut bytes_slice = &*bytes;
        let slice = &mut bytes_slice;

        let length_varint = VarInt::decode_partial(slice).context("failed to decode varint")?;
        let length = usize::try_from(length_varint).context("failed to convert varint to usize")?;

        let slice_len = bytes_slice.len();

        ensure!(
            length == slice_len,
            "length mismatch: var int length {}, got pkt length {}",
            length,
            slice_len
        );

        let (result, _) = self.stream.write_all(bytes).await;
        result?;
        // self.stream.flush().await?; // todo: remove

        Ok(())
    }

    #[instrument(skip(self, tx))]
    async fn server_process(
        mut self,
        id: usize,
        tx: flume::Sender<ClientConnection>,
    ) -> anyhow::Result<()> {
        // self.stream.set_nodelay(true)?;

        let ip = self.stream.peer_addr()?;

        debug!("connection from {ip}");

        let HandshakeC2s {
            protocol_version,
            next_state,
            ..
        } = self.recv_packet().await?;

        let version = protocol_version.0;

        ensure!(
            protocol_version.0 == PROTOCOL_VERSION,
            "expected protocol version {PROTOCOL_VERSION}, got {version}"
        );

        match next_state {
            HandshakeNextState::Status => self.server_status().await?,
            HandshakeNextState::Login => self.server_login(tx).await?,
        }

        Ok(())
    }

    async fn server_login(mut self, tx: flume::Sender<ClientConnection>) -> anyhow::Result<()> {
        debug!("[[start login phase]]");

        // first
        let LoginHelloC2s { username, .. } = self.recv_packet().await?;

        // todo: use
        // let _profile_id = profile_id.context("missing profile id")?;

        let username: Box<str> = Box::from(username.0);

        let uuid = offline_uuid(&username)?; // todo: random

        let packet = LoginSuccessS2c {
            uuid,
            username: Bounded::from(&*username),
            properties: Cow::default(),
        };

        // second
        self.send_packet(&packet).await?;

        // bound at 1024 packets
        let (s2c_tx, s2c_rx) = flume::unbounded();

        let raw_fd = self.stream.as_raw_fd();
        let (read, write) = self.stream.into_split();

        let speed = Arc::new(AtomicU32::new(DEFAULT_SPEED));

        let writer_comm = WriterComm {
            tx: s2c_tx,
            enc: self.enc,
            speed_mib_per_second: Arc::clone(&speed),
        };

        let mut io_write = IoWrite { write, raw_fd };

        let mut io_read = IoRead {
            stream: read,
            dec: self.dec,
        };

        debug!("Finished handshake for {username}");

        monoio::spawn(async move {
            while let Ok(packet) = io_read.recv_packet_raw().await {
                GLOBAL_C2S_PACKETS
                    .lock()
                    .push(UserPacketFrame { packet, user: uuid });
            }
        });

        monoio::spawn(async move {
            let mut past_queued_send = 0;
            let mut past_instant = Instant::now();
            while let Ok(bytes) = s2c_rx.recv_async().await {
                let len = bytes.len();

                trace!("got byte len: {len}");

                if let Err(e) = io_write.send_packet(bytes).await {
                    error!("Error sending packet: {e} ... {e:?}");
                    break;
                }
                let elapsed = past_instant.elapsed();

                // todo: clarify why 1 second?
                if elapsed > Duration::from_secs(1) {
                    let queued_send = io_write.queued_send();

                    let elapsed_seconds = elapsed.as_secs_f32();

                    // precision
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "precision loss is not an issue"
                    )]
                    let queued_send_difference = { (past_queued_send - queued_send) as f32 };

                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "speed is always positive"
                    )]
                    {
                        speed.store(
                            (queued_send_difference / elapsed_seconds) as u32,
                            Ordering::Relaxed,
                        );
                    }
                    past_queued_send = io_write.queued_send();
                    past_instant = Instant::now();
                } else {
                    // This will make the estimated speed slightly lower than the actual speed, but
                    // it makes measuring speed more practical because the server will send packets
                    // to the client more often than 1 second
                    {
                        past_queued_send += libc::c_int::try_from(len).unwrap();
                    }
                }
            }
        });

        let packets = Packets {
            writer: writer_comm,
        };

        let conn = ClientConnection {
            packets,
            name: username,
            uuid,
        };

        tx.send(conn).unwrap();

        Ok(())
    }

    async fn server_status(mut self) -> anyhow::Result<()> {
        debug!("status");
        let status::QueryRequestC2s = self.recv_packet().await?;

        let player_count = SHARED.player_count.load(Ordering::Relaxed);

        //  64x64 pixels image
        let bytes = include_bytes!("saul.png");
        let base64 = base64::engine::GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            base64::engine::general_purpose::NO_PAD,
        );

        let result = base64.encode(bytes);

        // data:image/png;base64,{result}
        let favicon = format!("data:image/png;base64,{result}");

        // https://wiki.vg/Server_List_Ping#Response
        let json = json!({
            "version": {
                "name": MINECRAFT_VERSION,
                "protocol": PROTOCOL_VERSION,
            },
            "players": {
                "online": player_count,
                "max": config::CONFIG.max_players,
                "sample": [],
            },
            "favicon": favicon,
            "description": config::CONFIG.server_desc.clone(),
        });

        let json = serde_json::to_string_pretty(&json)?;

        let send = status::QueryResponseS2c { json: &json };

        self.send_packet(&send).await?;

        debug!("wrote status response");

        // ping
        let status::QueryPingC2s { payload } = self.recv_packet().await?;

        debug!("read ping {}", payload);

        let pong = status::QueryPongS2c { payload };
        self.send_packet(&pong).await?;

        Ok(())
    }
}

async fn print_errors(future: impl core::future::Future<Output = anyhow::Result<()>>) {
    if let Err(err) = future.await {
        error!("{:?}", err);
    }
}

pub static GLOBAL_C2S_PACKETS: spin::Mutex<Vec<UserPacketFrame>> = spin::Mutex::new(Vec::new());

#[instrument(skip_all)]
async fn io_thread(tx: flume::Sender<ClientConnection>, address: impl ToSocketAddrs) {
    let listener = match TcpListener::bind(address) {
        Ok(listener) => listener,
        Err(e) => {
            error!("failed to bind: {e}");
            return;
        }
    };

    let id = 0;

    // accept incoming connections
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                warn!("accept failed: {e} ... {e:?}");
                continue;
            }
        };

        let process = Io::new(stream);

        let tx = tx.clone();

        let action = process.server_process(id, tx);
        let action = print_errors(action);

        monoio::spawn(action);
    }
}

pub fn start_io_thread(
    shutdown: flume::Receiver<()>,
    address: impl ToSocketAddrs + Send + Sync + 'static,
) -> anyhow::Result<flume::Receiver<ClientConnection>> {
    let (connection_tx, connection_rx) = flume::unbounded();

    std::thread::Builder::new()
        .name("io".to_string())
        .spawn(move || {
            let mut runtime = monoio::RuntimeBuilder::<monoio::FusionDriver>::new()
                .build()
                .unwrap();

            runtime.block_on(async move {
                let run = io_thread(connection_tx, address);
                let shutdown = shutdown.recv_async();

                monoio::select! {
                    () = run => {},
                    _ = shutdown => {},
                }
            });
        })
        .context("failed to spawn io thread")?;

    Ok(connection_rx)
}

pub fn registry_codec_raw(codec: &RegistryCodec) -> anyhow::Result<Compound> {
    // codec.cached_codec.clear();

    let mut compound = Compound::default();

    for (reg_name, reg) in &codec.registries {
        let mut value = vec![];

        for (id, v) in reg.iter().enumerate() {
            let id = i32::try_from(id).context("id too large")?;
            value.push(compound! {
                "id" => id,
                "name" => v.name.as_str(),
                "element" => v.element.clone(),
            });
        }

        let registry = compound! {
            "type" => reg_name.as_str(),
            "value" => List::Compound(value),
        };

        compound.insert(reg_name.as_str(), registry);
    }

    Ok(compound)
}
