//! All the networking related code.

#![expect(clippy::future_not_send, reason = "monoio is not Send")]

use std::{
    borrow::Cow,
    io,
    io::ErrorKind,
    net::ToSocketAddrs,
    os::fd::{AsRawFd, RawFd},
    ptr::addr_of_mut,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{ensure, Context};
use arrayvec::ArrayVec;
use base64::Engine;
use bytes::BytesMut;
use derive_build::Build;
use evenio::prelude::Component;
use libc::iovec;
use monoio::{
    buf::IoVecBuf,
    io::{
        AsyncReadRent, AsyncWriteRent, AsyncWriteRentExt, OwnedReadHalf, OwnedWriteHalf, Splitable,
    },
    net::{TcpListener, TcpStream},
    FusionRuntime,
};
use serde_json::json;
use sha2::Digest;
use tracing::{debug, error, info, instrument, trace, warn};
use valence_protocol::{
    decode::PacketFrame,
    packets::{
        handshaking::{handshake_c2s::HandshakeNextState, HandshakeC2s},
        login,
        login::{LoginHelloC2s, LoginSuccessS2c},
        status,
    },
    uuid::Uuid,
    Bounded, CompressionThreshold, Decode, Encode, Ident, PacketDecoder, PacketEncoder, VarInt,
};

use crate::{config, global};

/// Default MiB/s threshold before we start to limit the sending of some packets.
const DEFAULT_SPEED: u32 = 1024 * 1024;

/// The maximum number of buffers a vectored write can have.
const MAX_VECTORED_WRITE_BUFS: usize = 16;

/// How long we wait from when we get the first buffer to when we start sending all of the ones we have collected.
/// This is closely related to [`MAX_VECTORED_WRITE_BUFS`].
const WRITE_DELAY: Duration = Duration::from_millis(1);

// todo: make sure this is the safe as page size. (I think this is usually 4096)
/// How much we expand our read buffer each time a packet is too large.
const READ_BUF_SIZE: usize = 4096;

/// The Minecraft protocol version this library currently targets.
pub const PROTOCOL_VERSION: i32 = 763;

/// The maximum number of bytes that can be sent in a single packet.
pub const MAX_PACKET_SIZE: usize = 0x001F_FFFF;

/// The stringified name of the Minecraft version this library currently
/// targets.
pub const MINECRAFT_VERSION: &str = "1.20.1";

/// Get a [`Uuid`] based on the given user's name.
fn offline_uuid(username: &str) -> anyhow::Result<Uuid> {
    let digest = sha2::Sha256::digest(username);

    #[expect(clippy::indexing_slicing, reason = "sha256 is always 32 bytes")]
    let slice = &digest[..16];

    Uuid::from_slice(slice).context("failed to create uuid")
}

/// Sent from the I/O thread when it has established a connection with the player through a handshake
pub struct ClientConnection {
    /// The local encoder used by that player
    pub encoder: Encoder,
    /// Send channel to send bytes to the player.
    pub tx: flume::Sender<bytes::Bytes>,
    /// The name of the player.
    pub name: Box<str>,
    /// The UUID of the player.
    pub uuid: Uuid,
}

/// Used during handshake to communicate with the client.
pub struct Io {
    /// The stream of bytes from the client.
    stream: TcpStream,
    /// The decoding buffer and logic
    dec: PacketDecoder,
    /// The encoding buffer and logic
    enc: PacketEncoder,
    /// The latest frame received from the client.
    frame: PacketFrame,
    /// The shared state between the ECS framework and the I/O thread.
    shared: Arc<global::Shared>,
}

/// The writer for the connection once handshake is complete.
pub struct IoWrite {
    /// The stream of bytes to the client.
    write: OwnedWriteHalf<TcpStream>,
    /// The raw file descriptor of the stream.
    raw_fd: RawFd,
}

/// The reader for the connection once handshake is complete.
pub struct IoRead {
    /// The stream of bytes from the client.
    stream: OwnedReadHalf<TcpStream>,
    /// The decoding buffer and logic
    dec: PacketDecoder,
}

/// The connection to the client that allows for sending packets.
#[derive(Component, Build)]
pub struct Connection {
    /// The tx channel that send bytes to be received by the client.
    #[required]
    tx: flume::Sender<bytes::Bytes>,
}

impl Connection {
    /// Returns true if the connection is closed.
    pub fn is_closed(&self) -> bool {
        self.tx.is_disconnected()
    }

    /// Sends a bunch of bytes to the client.
    pub fn send(&self, bytes: bytes::Bytes) -> anyhow::Result<()> {
        trace!("send raw bytes");
        self.tx.send(bytes)?;
        Ok(())
    }
}

#[derive(Component)]
pub struct Encoder {
    /// The encoding buffer and logic
    enc: PacketEncoder,

    /// If we should clear the `enc` allocation once we are done sending it off.
    ///
    /// In the future, perhaps we will have a global buffer if it is performant enough.
    deallocate_on_process: bool,
}

impl Encoder {
    /// The [`Encoder`] will deallocate its allocation when it is done sending it off.
    pub fn deallocate_on_process(&mut self) {
        self.deallocate_on_process = true;
    }

    /// Takes all bytes from the encoding buffer and returns them.
    pub fn take(&mut self, compression: CompressionThreshold) -> bytes::Bytes {
        let result = self.enc.take().freeze();

        if self.deallocate_on_process {
            // to clear the allocation, we need to create a new encoder
            self.enc = PacketEncoder::new();
            self.enc.set_compression(compression);
            self.deallocate_on_process = false;
        }

        result
    }

    /// A mutable reference to the raw encoder
    pub fn inner_mut(&mut self) -> &mut PacketEncoder {
        &mut self.enc
    }

    /// Encode a packet.
    pub fn encode<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        self.enc.append_packet(pkt)?;

        Ok(())
    }

    /// This sends the bytes to the connection.
    /// [`PacketEncoder`] can have compression enabled.
    /// One must make sure the bytes are pre-compressed if compression is enabled.
    pub fn append(&mut self, bytes: &[u8]) {
        trace!("send raw bytes");
        self.enc.append_bytes(bytes);
    }
}

/// An incoming packet that is tied to a user.
pub struct UserPacketFrame {
    /// The raw packet
    pub packet: PacketFrame,
    /// The UUID of the user
    /// todo: user is 128 bits, could we get away with fewer?
    pub user: Uuid,
}

impl IoRead {
    /// Receives a packet from the connection.
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

/// A buffer which can be used with `writev`.
struct Buf {
    /// The [`iovec`]s to write
    iovecs: ArrayVec<iovec, MAX_VECTORED_WRITE_BUFS>,
    /// Reference to the original [`bytes::Bytes`] used to create the [`iovec`]s. This is important so they do
    /// not get freed.
    _ref: ArrayVec<bytes::Bytes, MAX_VECTORED_WRITE_BUFS>,
    /// The index of the [`iovec`] that is currently being written to.
    idx: usize,
}

// SAFETY: The underlying data will live longer than this struct.
unsafe impl IoVecBuf for Buf {
    fn read_iovec_ptr(&self) -> *const iovec {
        unsafe { self.iovecs.as_ptr().add(self.idx) }
    }

    fn read_iovec_len(&self) -> usize {
        self.iovecs.len() - self.idx
    }
}

impl Buf {
    /// Given a result of `writev`, this will allow for progressing the buffer by the number of bytes written.
    ///
    /// If the number of bytes is the entire buffer, then `None` is returned.
    fn progress(mut self, mut len: usize) -> Option<Self> {
        loop {
            let vec = self.iovecs.get_mut(self.idx)?;
            let iov_len = vec.iov_len;

            // this is perhaps not strictly needed, but we should not be writing zero-length iovecs
            // anyway. It probably hurts performance.
            debug_assert!(iov_len > 0);

            if len >= iov_len {
                len -= iov_len;
                self.idx += 1;
                continue;
            }

            vec.iov_len -= len;
            vec.iov_base = unsafe { vec.iov_base.add(len) };

            return Some(self);
        }
    }
}

impl IoWrite {
    /// Given a vector of bytes, this will send them to the connection using `writev`.
    async fn send_data(
        &mut self,
        bytes: ArrayVec<bytes::Bytes, MAX_VECTORED_WRITE_BUFS>,
    ) -> io::Result<()> {
        let iovecs = bytes
            .iter()
            .map(|bytes| iovec {
                #[expect(clippy::as_ptr_cast_mut, reason = "The pointer won't be written to")]
                iov_base: bytes.as_ptr() as *mut _,
                iov_len: bytes.len(),
            })
            .collect();

        let mut buf_on = Some(Buf {
            iovecs,
            _ref: bytes,
            idx: 0,
        });

        while let Some(buf) = buf_on.take() {
            let (result, buf) = self.write.writev(buf).await;
            match result {
                Ok(len_read) => {
                    buf_on = buf.progress(len_read);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    buf_on = Some(buf);
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

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
                let result = libc::ioctl(self.raw_fd, libc::TIOCOUTQ, addr_of_mut!(value));
                if result == -1 {
                    let err = io::Error::last_os_error();
                    panic!("getsockopt failed: {err}");
                }
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
                let result = libc::getsockopt(
                    self.raw_fd,
                    libc::SOL_SOCKET,
                    libc::SO_NWRITE,
                    addr_of_mut!(value).cast(),
                    addr_of_mut!(len),
                );

                if result == -1 {
                    let err = io::Error::last_os_error();
                    panic!("getsockopt failed: {err}");
                }
            }
            value
        }

        // TODO: Support getting queued send for other OS
    }
}

impl Io {
    /// Receives a packet from the connection.
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

    /// Creates a new [`Io`] with the given stream.
    fn new(stream: TcpStream, shared: Arc<global::Shared>) -> Self {
        // TCP_NODELAY is enabled because the code already has a WRITE_DELAY
        if let Err(e) = stream.set_nodelay(true) {
            warn!("set_nodelay failed: {e}");
        }

        let enc = PacketEncoder::default();
        let dec = PacketDecoder::default();

        Self {
            stream,
            dec,
            enc,
            frame: PacketFrame {
                id: 0,
                body: BytesMut::new(),
            },
            shared,
        }
    }

    /// Send a packet to the connection.
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

        Ok(())
    }

    #[instrument(skip(self, tx))]
    async fn process_new_connection(
        mut self,
        id: usize,
        tx: flume::Sender<ClientConnection>,
    ) -> anyhow::Result<()> {
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

    #[instrument(skip(self, tx))]
    async fn server_login(mut self, tx: flume::Sender<ClientConnection>) -> anyhow::Result<()> {
        debug!("[[start login phase]]");

        // first
        let LoginHelloC2s { username, .. } = self.recv_packet().await?;

        // todo: use
        // let _profile_id = profile_id.context("missing profile id")?;

        let username = username.0;

        // trim username to 10 chars
        let username_len = std::cmp::min(username.len(), 10);
        let username = &username[..username_len];

        // add 2 random chars to the end of the username
        let username = format!(
            "{}-{}{}",
            username,
            fastrand::alphanumeric(),
            fastrand::alphanumeric()
        );

        let uuid = offline_uuid(&username)?;

        let compression_level = self.shared.compression_level;
        if compression_level.0 > 0 {
            self.send_packet(&login::LoginCompressionS2c {
                threshold: compression_level.0.into(),
            })
            .await?;

            self.enc.set_compression(compression_level);
            self.dec.set_compression(compression_level);

            debug!("compression level set to {}", compression_level.0);
        }

        let packet = login::LoginQueryRequestS2c {
            message_id: VarInt(42), // todo
            channel: Ident::new("voicechat").unwrap(),
            data: Bounded::default(),
        };

        // todo: is this corerct?
        self.send_packet(&packet).await?;
        
        let response: login::LoginQueryResponseC2s = self.recv_packet().await?;
        
        println!("{response:?}");

        let packet = LoginSuccessS2c {
            uuid,
            username: Bounded::from(&*username),
            properties: Cow::default(),
        };

        // second
        self.send_packet(&packet).await?;

        // bound at 1024 packets
        let (s2c_tx, s2c_rx) = flume::unbounded::<bytes::Bytes>();

        let raw_fd = self.stream.as_raw_fd();
        let (read, write) = self.stream.into_split();

        let speed = Arc::new(AtomicU32::new(DEFAULT_SPEED));

        let encoder = Encoder {
            enc: self.enc,
            deallocate_on_process: false,
        };

        let mut io_write = IoWrite { write, raw_fd };

        let mut io_read = IoRead {
            stream: read,
            dec: self.dec,
        };

        debug!("Finished handshake for {username}");

        monoio::spawn(async move {
            while let Ok(packet) = io_read.recv_packet_raw().await {
                tracing::info_span!("adding global packets").in_scope(|| {
                    GLOBAL_C2S_PACKETS
                        .lock()
                        .push(UserPacketFrame { packet, user: uuid });
                });
            }
        });

        monoio::spawn(async move {
            let mut past_queued_send = 0;
            let mut past_instant = Instant::now();
            while let Ok(bytes) = s2c_rx.recv_async().await {
                let mut bytes_buf = ArrayVec::<_, MAX_VECTORED_WRITE_BUFS>::new();
                bytes_buf.push(bytes);

                let mut already_delayed = false;

                while !bytes_buf.is_full() {
                    // Try getting more bytes if it's already in the channel before sending data
                    if let Ok(bytes) = s2c_rx.try_recv() {
                        bytes_buf.push(bytes);
                    } else if already_delayed {
                        // This write request has already been delayed, so send the data now
                        break;
                    } else {
                        // Wait for WRITE_DELAY and then check if any more packets are queued
                        monoio::time::sleep(WRITE_DELAY).await;
                        already_delayed = true;
                    }
                }

                if bytes_buf.is_full() {
                    warn!(
                        "bytes_buf is full; consider increasing MAX_VECTORED_WRITE_BUFS for \
                         better performance"
                    );
                }

                let len = bytes_buf.iter().map(bytes::Bytes::len).sum::<usize>();

                trace!("got byte len: {len}");

                if let Err(e) = io_write.send_data(bytes_buf).await {
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

        let conn = ClientConnection {
            encoder,
            tx: s2c_tx,
            name: username.into_boxed_str(),
            uuid,
        };

        tx.send(conn).unwrap();

        Ok(())
    }

    #[instrument(skip(self))]
    async fn server_status(mut self) -> anyhow::Result<()> {
        debug!("status");
        let status::QueryRequestC2s = self.recv_packet().await?;

        let player_count = self.shared.player_count.load(Ordering::Relaxed);

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

/// logs all errors that occur when writing to the connection.
///
/// this is far better than panicking because it allows us to log the error and continue handling other
/// clients/connections
async fn print_errors(future: impl core::future::Future<Output = anyhow::Result<()>>) {
    if let Err(err) = future.await {
        error!("{:?}", err);
    }
}

/// All incoming packets.
///
/// todo: `GLOBAL_CTS_PACKETS` there is a lot of room for improvement and experimentation here.
/// Perhaps could implement some type of `MultiMap` such that a certain player can have their packets queried
/// easier.
/// This would be especially useful when we are processing packets in parallel.
///
/// Also, this should not be a static ideally but instead an `Arc` probably in [`global::Shared`].
pub static GLOBAL_C2S_PACKETS: spin::Mutex<Vec<UserPacketFrame>> = spin::Mutex::new(Vec::new());

#[instrument(skip_all)]
async fn main_loop(
    tx: flume::Sender<ClientConnection>,
    address: impl ToSocketAddrs,
    shared: Arc<global::Shared>,
) {
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

        let process = Io::new(stream, shared.clone());

        let tx = tx.clone();

        let action = process.process_new_connection(id, tx);
        let action = print_errors(action);

        monoio::spawn(action);
    }
}

/// Initializes the I/O thread.
pub fn init_io_thread(
    shutdown: flume::Receiver<()>,
    address: impl ToSocketAddrs + Send + Sync + 'static,
    shared: Arc<global::Shared>,
) -> anyhow::Result<flume::Receiver<ClientConnection>> {
    let (connection_tx, connection_rx) = flume::unbounded();

    std::thread::Builder::new()
        .name("io".to_string())
        .spawn(move || {
            let mut runtime = monoio::RuntimeBuilder::<monoio::FusionDriver>::new()
                .enable_timer()
                .build()
                .unwrap();

            match &runtime {
                #[cfg(target_os = "linux")]
                FusionRuntime::Uring(_) => {
                    info!("monoio is using io_uring runtime");
                }
                FusionRuntime::Legacy(_) => {
                    info!("monoio is using legacy runtime");
                }
            }

            runtime.block_on(async move {
                let run = main_loop(connection_tx, address, shared);
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
