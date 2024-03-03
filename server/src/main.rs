#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{borrow::Cow, collections::BTreeSet, io, io::ErrorKind};

use anyhow::{ensure, Context};
use azalea_buf::McBufWritable;
use bytes::BytesMut;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::{debug, error, info, instrument};
use valence::{
    math::DVec3, protocol as valence_protocol,
    protocol::packets::play::player_position_look_s2c::PlayerPositionLookFlags,
    registry::RegistryCodec, BlockPos,
};
use valence_protocol::{
    decode::PacketFrame,
    game_mode::OptGameMode,
    nbt::Compound,
    packets::{
        handshaking::{handshake_c2s::HandshakeNextState, HandshakeC2s},
        login::{LoginHelloC2s, LoginSuccessS2c},
        status,
    },
    Bounded, ChunkPos, Decode, Encode, GameMode, Ident, PacketDecoder, PacketEncoder, VarInt,
};

const READ_BUF_SIZE: usize = 4096;

/// The Minecraft protocol version this library currently targets.
pub const PROTOCOL_VERSION: i32 = 763;

/// The stringified name of the Minecraft version this library currently
/// targets.
pub const MINECRAFT_VERSION: &str = "1.20.1";

struct Io {
    stream: TcpStream,
    dec: PacketDecoder,
    enc: PacketEncoder,
    frame: PacketFrame,
}

impl Io {
    pub async fn recv_packet<'a, P>(&'a mut self) -> anyhow::Result<P>
    where
        P: valence_protocol::Packet + Decode<'a>,
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

        self.stream.write_all(&bytes).await?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn process(mut self, id: usize) -> anyhow::Result<()> {
        self.stream.set_nodelay(true)?;

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
            HandshakeNextState::Status => self.status().await?,
            HandshakeNextState::Login => self.login().await?,
        }

        Ok(())
    }

    async fn login(mut self) -> anyhow::Result<()> {
        debug!("login");

        // first
        let LoginHelloC2s {
            username,
            profile_id,
        } = self.recv_packet().await?;

        let profile_id = profile_id.context("missing profile id")?;

        debug!("username: {username}");
        debug!("profile id: {profile_id:?}");

        // let username: Box<str> = Box::from(username.0);
        let username = "Emerald_Explorer";

        let packet = LoginSuccessS2c {
            uuid: profile_id,
            username: Bounded::from(username),
            properties: Cow::Borrowed(&[]),
        };

        debug!("sending {packet:?}");

        // second
        self.send_packet(&packet).await?;

        self.main_loop().await?;

        Ok(())
    }

    async fn main_loop(mut self) -> anyhow::Result<()> {
        use valence_protocol::packets::play;
        info!("main loop");

        let overworld: Ident<Cow<'static, str>> = "minecraft:overworld".try_into()?;
        let set: BTreeSet<_> = std::iter::once(overworld).collect();

        // recv ack

        let dimension_names = Cow::Owned(set);

        let default_codec = RegistryCodec::default();

        let registry_codec = default_codec.cached_codec();

        let pkt = play::GameJoinS2c {
            entity_id: 123,
            is_hardcore: false,
            dimension_names,
            registry_codec: Cow::Borrowed(registry_codec),
            max_players: 10_000.into(),
            view_distance: 10.into(),
            simulation_distance: 10.into(),
            reduced_debug_info: false,
            enable_respawn_screen: false,
            dimension_name: "minecraft:overworld".try_into()?,
            hashed_seed: 0,
            game_mode: GameMode::Survival,
            is_flat: false,
            last_death_location: None,
            portal_cooldown: 0.into(),
            previous_game_mode: OptGameMode(None),
            dimension_type_name: "minecraft:overworld".try_into()?,
            is_debug: false,
        };

        self.send_packet(&pkt).await?;

        info!("wrote login");

        let mut chunk = azalea_world::Chunk::default();

        #[allow(clippy::indexing_slicing)]
        let first_section = &mut chunk.sections[0];

        let states = &mut first_section.states;

        for x in 0..16 {
            for z in 0..16 {
                let id: u32 = 2;
                states.set(x, 0, z, id);
            }
        }

        let mut bytes = Vec::new();

        chunk.write_into(&mut bytes)?;

        let chunk = play::ChunkDataS2c {
            pos: ChunkPos::new(0, 0),
            heightmaps: Cow::Owned(Compound::new()),
            blocks_and_biomes: &bytes,
            block_entities: Cow::Borrowed(&[]),
            sky_light_mask: Cow::Borrowed(&[]),
            block_light_mask: Cow::Borrowed(&[]),
            empty_sky_light_mask: Cow::Borrowed(&[]),
            empty_block_light_mask: Cow::default(),
            sky_light_arrays: Cow::default(),
            block_light_arrays: Cow::Borrowed(&[]),
        };

        self.send_packet(&chunk).await?;
        info!("wrote chunk");

        let mut flags = PlayerPositionLookFlags::default();

        flags.set_x(true);
        flags.set_y(true);
        flags.set_z(true);

        // Synchronize Player Position
        // Set Player Position and Rotation
        let pos = play::PlayerPositionLookS2c {
            position: DVec3::new(0.0, 2.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            flags,
            teleport_id: 1.into(),
        };
        self.send_packet(&pos).await?;
        info!("wrote pos");

        // Spawn
        let spawn = play::PlayerSpawnPositionS2c {
            position: BlockPos::default(),
            angle: 3.0,
        };
        self.send_packet(&spawn).await?;
        info!("wrote spawn");

        loop {
            let raw = self.recv_packet_raw().await?;

            info!("read {raw:?}");
        }
    }

    async fn status(mut self) -> anyhow::Result<()> {
        debug!("status");
        let status::QueryRequestC2s = self.recv_packet().await?;

        let json = json!({
            "version": {
                "name": MINECRAFT_VERSION,
                "protocol": PROTOCOL_VERSION,
            },
            "players": {
                "online": 0,
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
