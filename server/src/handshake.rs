#![allow(unused)]
use std::{borrow::Cow, collections::BTreeSet, io, io::ErrorKind};

use anyhow::{ensure, Context};
use bytes::BytesMut;
use monoio::{
    io::{
        AsyncReadRent, AsyncWriteRent, AsyncWriteRentExt, OwnedReadHalf, OwnedWriteHalf, Splitable,
    },
    net::{TcpListener, TcpStream},
};
use serde_json::json;
use sha2::Digest;
use signal_hook::iterator::Signals;
use tracing::{debug, error, info, warn};
use valence_protocol::{
    decode::PacketFrame,
    game_mode::OptGameMode,
    ident,
    nbt::{compound, Compound, List},
    packets::{
        handshaking::{handshake_c2s::HandshakeNextState, HandshakeC2s},
        login::{LoginHelloC2s, LoginSuccessS2c},
        play::GameJoinS2c,
        status,
    },
    uuid::Uuid,
    Bounded, Decode, Encode, GameMode, Ident, PacketDecoder, PacketEncoder, VarInt,
};
use valence_registry::{BiomeRegistry, RegistryCodec};

use crate::GLOBAL;

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
    #[allow(clippy::indexing_slicing)]
    Uuid::from_slice(&sha2::Sha256::digest(username)[..16]).map_err(Into::into)
}

pub struct Io {
    stream: TcpStream,
    dec: PacketDecoder,
    enc: PacketEncoder,
    frame: PacketFrame,
}

pub struct IoWrite {
    write: OwnedWriteHalf<TcpStream>,
}

pub struct IoRead {
    stream: OwnedReadHalf<TcpStream>,
    dec: PacketDecoder,
}

pub struct WriterComm {
    tx: flume::Sender<BytesMut>,
    enc: PacketEncoder,
}

type ReaderComm = flume::Receiver<PacketFrame>;

impl WriterComm {
    pub(crate) fn send_packet<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: valence_protocol::Packet + Encode,
    {
        self.enc.append_packet(pkt)?;
        let bytes = self.enc.take();

        let mut bytes_slice = &*bytes;
        let slice = &mut bytes_slice;
        #[allow(clippy::cast_sign_loss)]
        let length = VarInt::decode_partial(slice)? as usize;

        let slice_len = bytes_slice.len();

        ensure!(
            length == slice_len,
            "length mismatch: var int length {}, got pkt length {}",
            length,
            slice_len
        );

        self.tx.send(bytes)?;

        Ok(())
    }

    pub fn send_game_join_packet(&mut self) -> anyhow::Result<()> {
        // recv ack

        let codec = RegistryCodec::default();

        let registry_codec = registry_codec_raw(&codec)?;

        let dimension_names: BTreeSet<Ident<Cow<str>>> = codec
            .registry(BiomeRegistry::KEY)
            .iter()
            .map(|value| value.name.as_str_ident().into())
            .collect();

        let dimension_name = ident!("overworld");
        // let dimension_name: Ident<Cow<str>> = chunk_layer.dimension_type_name().into();

        let pkt = GameJoinS2c {
            entity_id: 0,
            is_hardcore: false,
            dimension_names: Cow::Owned(dimension_names),
            registry_codec: Cow::Borrowed(&registry_codec),
            max_players: 10_000.into(),
            view_distance: 32.into(), // max view distance
            simulation_distance: 10.into(),
            reduced_debug_info: false,
            enable_respawn_screen: false,
            dimension_name: dimension_name.into(),
            hashed_seed: 0,
            game_mode: GameMode::Creative,
            is_flat: false,
            last_death_location: None,
            portal_cooldown: 60.into(),
            previous_game_mode: OptGameMode(Some(GameMode::Creative)),
            dimension_type_name: "minecraft:overworld".try_into()?,
            is_debug: false,
        };

        self.send_packet(&pkt)?;

        Ok(())
    }
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
    pub(crate) async fn send_packet(&mut self, bytes: BytesMut) -> anyhow::Result<()> {
        let (result, _) = self.write.write_all(bytes).await;

        result?;

        // todo: is flush needed?
        self.write.flush().await?;

        Ok(())
    }
}

pub struct Packets {
    pub writer: WriterComm,
    pub reader: ReaderComm,
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
                // info!("read packet {decode:#?}");
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
        #[allow(clippy::cast_sign_loss)]
        let length = VarInt::decode_partial(slice).unwrap() as usize;

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

    async fn server_process(mut self, id: usize, tx: flume::Sender<Packets>) -> anyhow::Result<()> {
        // self.stream.set_nodelay(true)?;

        info!("connection id {id}");

        let ip = self.stream.peer_addr()?;

        info!("connection from {ip}");

        let HandshakeC2s {
            protocol_version,
            server_port,
            next_state,
            ..
        } = self.recv_packet().await?;

        let version = protocol_version.0;

        ensure!(
            protocol_version.0 == PROTOCOL_VERSION,
            "expected protocol version {PROTOCOL_VERSION}, got {version}"
        );
        ensure!(server_port == 25565, "expected server port 25565");

        match next_state {
            HandshakeNextState::Status => self.server_status().await?,
            HandshakeNextState::Login => self.server_login(tx).await?,
        }

        Ok(())
    }

    async fn server_login(mut self, tx: flume::Sender<Packets>) -> anyhow::Result<()> {
        debug!("[[start login phase]]");

        // first
        let LoginHelloC2s {
            username,
            profile_id,
        } = self.recv_packet().await?;

        // todo: use
        let _profile_id = profile_id.context("missing profile id")?;

        let username: Box<str> = Box::from(username.0);

        let packet = LoginSuccessS2c {
            uuid: offline_uuid(&username)?,
            username: Bounded::from(&*username),
            properties: Cow::default(),
        };

        // second
        self.send_packet(&packet).await?;

        let (s2c_tx, s2c_rx) = flume::unbounded();
        let (c2s_tx, c2s_rx) = flume::unbounded();

        let (read, write) = self.stream.into_split();

        let writer_comm = WriterComm {
            tx: s2c_tx,
            enc: self.enc,
        };

        let reader_comm = c2s_rx;

        let mut io_write = IoWrite { write };

        let mut io_read = IoRead {
            stream: read,
            dec: self.dec,
        };

        info!("Finished handshake for {username}");

        monoio::spawn(async move {
            // sleep 1 second
            // debug!("before");
            debug!("start receiving packets");
            while let Ok(raw) = io_read.recv_packet_raw().await {
                c2s_tx.send(raw).unwrap();
            }
        });

        monoio::spawn(async move {
            while let Ok(bytes) = s2c_rx.recv_async().await {
                io_write.send_packet(bytes).await.unwrap();
            }
        });

        let packets = Packets {
            writer: writer_comm,
            reader: reader_comm,
        };

        tx.send(packets).unwrap();

        Ok(())
    }

    async fn server_status(mut self) -> anyhow::Result<()> {
        debug!("status");
        let status::QueryRequestC2s = self.recv_packet().await?;

        let player_count = GLOBAL
            .player_count
            .load(std::sync::atomic::Ordering::Relaxed);

        let json = json!({
            "version": {
                "name": MINECRAFT_VERSION,
                "protocol": PROTOCOL_VERSION,
            },
            "players": {
                "online": player_count,
                "max": 10_000,
                "sample": [],
            },
            "description": "10k babyyyyy",
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

async fn run(tx: flume::Sender<Packets>) {
    // start socket 25565
    // todo: remove unwrap
    let addr = "0.0.0.0:25565";

    let listener = match TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(e) => {
            error!("failed to bind to {addr}: {e}");
            return;
        }
    };

    info!("listening on {addr}");

    let mut id = 0;

    // accept incoming connections
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            warn!("accept failed");
            continue;
        };

        info!("accepted connection {id}");

        let process = Io::new(stream);

        let tx = tx.clone();

        let action = process.server_process(id, tx);
        let action = print_errors(action);

        monoio::spawn(action);
        id += 1;
    }
}

pub fn server(shutdown: flume::Receiver<()>) -> flume::Receiver<Packets> {
    let (tx, rx) = flume::unbounded();

    std::thread::spawn(move || {
        let mut runtime = monoio::RuntimeBuilder::<monoio::FusionDriver>::new()
            // .enable_timer()
            .build()
            .unwrap();

        runtime.block_on(async move {
            let run = run(tx);
            let shutdown = shutdown.recv_async();

            monoio::select! {
                () = run => {},
                _ = shutdown => {},
            }
        });
    });

    rx
}

fn registry_codec_raw(codec: &RegistryCodec) -> anyhow::Result<Compound> {
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
