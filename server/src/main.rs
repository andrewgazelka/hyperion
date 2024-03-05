// #![allow(unused)]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{borrow::Cow, collections::BTreeSet, io, io::ErrorKind};

use anyhow::{ensure, Context};
use azalea_buf::McBufWritable;
use bytes::BytesMut;
use serde_json::json;
use sha2::Digest;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::{debug, error, info};
use valence_protocol::{
    decode::PacketFrame,
    game_mode::OptGameMode,
    ident,
    math::DVec3,
    nbt::{compound, Compound, List},
    packets::{
        handshaking::{handshake_c2s::HandshakeNextState, HandshakeC2s},
        login::{LoginHelloC2s, LoginSuccessS2c},
        play::{
            player_list_s2c::{PlayerListActions, PlayerListEntry},
            player_position_look_s2c::PlayerPositionLookFlags,
            FullC2s, SynchronizeTagsS2c,
        },
        status,
    },
    uuid::Uuid,
    BlockPos, Bounded, ChunkPos, Decode, Encode, GameMode, Ident, Packet, PacketDecoder,
    PacketEncoder, RawBytes, VarInt, VarLong,
};
use valence_registry::{BiomeRegistry, RegistryCodec, TagsRegistry};

const READ_BUF_SIZE: usize = 4096;

/// The Minecraft protocol version this library currently targets.
pub const PROTOCOL_VERSION: i32 = 763;

/// The stringified name of the Minecraft version this library currently
/// targets.
pub const MINECRAFT_VERSION: &str = "1.20.1";

fn offline_uuid(username: &str) -> anyhow::Result<Uuid> {
    #[allow(clippy::indexing_slicing)]
    Uuid::from_slice(&sha2::Sha256::digest(username)[..16]).map_err(Into::into)
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
            let mut buf = self.dec.take_capacity();

            let bytes_read = self.stream.read_buf(&mut buf).await?;
            if bytes_read == 0 {
                return Err(io::Error::from(ErrorKind::UnexpectedEof).into());
            }

            debug!("read {bytes_read} bytes");

            // This should always be an O(1) unsplit because we reserved space earlier and
            // the call to `read_buf` shouldn't have grown the allocation.
            self.dec.queue_bytes(buf);
        }
    }

    pub async fn recv_packet_raw(&mut self) -> anyhow::Result<PacketFrame> {
        loop {
            if let Some(frame) = self.dec.try_next_packet()? {
                info!("read packet id {:#x}", frame.id);
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
        self.stream.flush().await?; // todo: remove

        // info!("wrote {pkt:#?}");

        Ok(())
    }

    // async fn client_process(mut self) -> anyhow::Result<()> {
    //     use valence_protocol::packets::handshaking;
    //
    //     let pkt = HandshakeC2s {
    //         protocol_version: PROTOCOL_VERSION.into(),
    //         server_address: "localhost:25565".into(),
    //         server_port: 25565,
    //         next_state: HandshakeNextState::Login,
    //     };
    //
    //     self.send_packet(&pkt).await?;
    //
    //     self.client_login().await?;
    //
    //     Ok(())
    // }

    // async fn client_login(mut self) -> anyhow::Result<()> {
    //     use valence_protocol::packets::login;
    //
    //     let pkt = login::LoginHelloC2s {
    //         username: "Emerald_Explorer".into(),
    //         profile_id: Some(Uuid::from_u128(0)),
    //     };
    //
    //     self.send_packet(&pkt).await?;
    //
    //     // let login::LoginCompressionS2c { threshold } = self.recv_packet().await?;
    //     //
    //     // let threshold = threshold.0;
    //     // info!("compression threshold {threshold}");
    //
    //     let pkt: login::LoginSuccessS2c = self.recv_packet().await?;
    //
    //     // self.client_read_loop().await?;
    //
    //     Ok(())
    // }

    // async fn client_read_loop(mut self) -> anyhow::Result<()> {
    //     let game_join: playGameJoinS2c = self.recv_packet().await?;
    //
    //     loop {
    //         let frame = self.recv_packet_raw().await?;
    //         let id = frame.id;
    //         // hex
    //         info!("read packet id {id:#x}");
    //     }
    // }

    async fn server_process(mut self, id: usize) -> anyhow::Result<()> {
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
            HandshakeNextState::Login => self.server_login().await?,
        }

        Ok(())
    }

    async fn server_login(mut self) -> anyhow::Result<()> {
        debug!("[[start login phase]]");

        // first
        let LoginHelloC2s {
            username,
            profile_id,
        } = self.recv_packet().await?;

        let profile_id = profile_id.context("missing profile id")?;

        let username: Box<str> = Box::from(username.0);

        let packet = LoginSuccessS2c {
            uuid: offline_uuid(&username)?,
            username: Bounded::from(&*username),
            properties: Cow::default(),
        };

        // second
        self.send_packet(&packet).await?;

        self.main_loop(profile_id, username.as_ref()).await?;

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn main_loop(mut self, uuid: Uuid, username: &str) -> anyhow::Result<()> {
        use valence_protocol::packets::play;
        info!("[[ start main phase ]]");

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

        let pkt = play::GameJoinS2c {
            entity_id: 0,
            is_hardcore: false,
            dimension_names: Cow::Owned(dimension_names),
            registry_codec: Cow::Borrowed(&registry_codec),
            max_players: 10_000.into(),
            view_distance: 10.into(),
            simulation_distance: 10.into(),
            reduced_debug_info: false,
            enable_respawn_screen: false,
            dimension_name: dimension_name.into(),
            hashed_seed: 0,
            game_mode: GameMode::Survival,
            is_flat: false,
            last_death_location: None,
            portal_cooldown: 60.into(),
            previous_game_mode: OptGameMode(Some(GameMode::Creative)),
            dimension_type_name: "minecraft:overworld".try_into()?,
            is_debug: false,
        };

        // self.enc.prepend_packet(&pkt)?;
        self.send_packet(&pkt).await?;

        // // todo:
        //
        // self.send_packet(&pkt).await?;
        //

        // // Spawn
        // let spawn = play::PlayerSpawnPositionS2c {
        //     position: BlockPos::default(),
        //     angle: 3.0,
        // };
        // self.send_packet(&spawn).await?;

        // todo: dont depend on this order
        info!("start read loop");

        // 15. client information
        let x: play::ClientSettingsC2s = self.recv_packet().await?;
        info!("read {x:#?}");

        // 16. hand held item
        self.send_packet(&play::UpdateSelectedSlotS2c { slot: 0 })
            .await?;

        // todo: maybe remove
        let mut bytes = Vec::new();
        // write varint with len 0
        VarInt(0).encode(&mut bytes)?;

        // Update Recipes
        self.send_packet(&play::SynchronizeRecipesS2c {
            recipes: RawBytes::from(bytes.as_slice()),
        })
        .await?;

        // 11. Plugin Message
        let pkt = self.recv_packet::<play::CustomPayloadC2s>().await?;
        info!("read {pkt:?}");

        // let brand = "minecraft:brand".into();
        // 14. Plugin Message: minecraft:brand with the client's brand (Optional)
        // self.send_packet(&play::CustomPayloadS2c {
        //     channel: Ident::from(brand),
        //     data: RawBytes::from("Valence".as_bytes()),
        // });

        // 18. Update Tags
        let registry = TagsRegistry::default();

        let pkt = SynchronizeTagsS2c {
            groups: Cow::Borrowed(&registry.registries),
        };
        self.send_packet(&pkt).await?;

        // 19. Entity Event

        // self.send_packet(play::EntityStatusS2c {
        //
        // }

        // 20. commands
        // todo: remove?
        // self.send_packet(&play::CommandTreeS2c {
        //     commands: vec![],
        //     root_index: VarInt::default(),
        // });

        // 21. Recipe

        // 22. Player position Player Info Update (Add Player)

        // player_uuid: Default::default(),
        // username: "",
        // properties: Default::default(),
        // chat_data: None,
        // listed: false,
        // ping: 0,
        // game_mode: Default::default(),
        // display_name: None,
        let player_list_entry = PlayerListEntry {
            player_uuid: uuid,
            username,
            // todo: impl
            properties: Cow::Borrowed(&[]),
            chat_data: None,
            listed: true, // show on player list
            ping: 0,      // ms
            game_mode: GameMode::Survival,
            display_name: None,
        };

        let entries = &[player_list_entry];

        // 23. Player Info (Add Player action)
        self.send_packet(&play::PlayerListS2c {
            actions: PlayerListActions::new().with_add_player(true),
            entries: Cow::Borrowed(entries),
        })
        .await?;

        // 24. Player Info (Add Player action)  (Update latency action)
        self.send_packet(&play::PlayerListS2c {
            actions: PlayerListActions::new().with_update_latency(true),
            entries: Cow::Borrowed(entries),
        })
        .await?;

        // 25. Set Center Chunk
        self.send_packet(&play::ChunkRenderDistanceCenterS2c {
            chunk_x: VarInt(0),
            chunk_z: VarInt(0),
        })
        .await?;

        // 26. Update light

        let mut pkt = play::LightUpdateS2c {
            chunk_x: VarInt::default(),
            chunk_z: VarInt::default(),
            sky_light_mask: Cow::default(),
            block_light_mask: Cow::default(),
            empty_sky_light_mask: Cow::default(),
            empty_block_light_mask: Cow::default(),
            sky_light_arrays: Cow::default(),
            block_light_arrays: Cow::default(),
        };

        // -16..=16
        for x in -16..=16 {
            for z in -16..=16 {
                pkt.chunk_x = VarInt(x);
                pkt.chunk_z = VarInt(z);
                self.send_packet(&pkt).await?;
            }
        }

        // 27. Chunk Data

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

        let mut chunk = play::ChunkDataS2c {
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

        for x in -16..=16 {
            for z in -16..=16 {
                chunk.pos = ChunkPos::new(x, z);
                self.send_packet(&chunk).await?;
            }
        }

        // 28. Initialize World Border
        self.send_packet(&play::WorldBorderInitializeS2c {
            x: 0.0,
            z: 0.0,
            old_diameter: 1000.0,
            new_diameter: 1000.0,
            duration_millis: VarLong::default(),
            portal_teleport_boundary: VarInt::default(),
            warning_blocks: VarInt::default(),
            warning_time: VarInt::default(),
        })
        .await?;

        // 29. S → C: Set Default Spawn Position (“home” spawn, not where the client will spawn on
        //     login)
        self.send_packet(&play::PlayerSpawnPositionS2c {
            position: BlockPos::new(0, 10, 0),
            angle: 0.0,
        })
        .await?;

        // 32. C → S: Set Player Position and Rotation (to confirm the spawn position)
        let pkt = self.recv_packet::<FullC2s>().await?;
        info!("32. {pkt:#?}");

        // Set Player Position
        let pkt = self.recv_packet::<play::PositionAndOnGroundC2s>().await?;
        info!("32. {pkt:#?}");

        // 30.Synchronize Player Position (Required, tells the client they're ready to spawn)
        self.send_packet(&play::PlayerPositionLookS2c {
            position: DVec3::new(0.0, 3.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            flags: PlayerPositionLookFlags::default(),
            teleport_id: 1.into(),
        })
        .await?;

        // Synchronize Player Position ******

        // raw
        let recv = loop {
            let frame = self.recv_packet_raw().await?;
            let id = frame.id;
            // if teleport confirm
            if id == play::TeleportConfirmC2s::ID {
                // 32
                break frame.decode::<play::TeleportConfirmC2s>()?;
            }

            // Set Player Position
            if id == play::PositionAndOnGroundC2s::ID {
                let pkt = frame.decode::<play::PositionAndOnGroundC2s>()?;
                info!("32. {pkt:#?}");
            }
        };

        let teleport_id = recv.teleport_id.0;
        info!("read {recv:#?}");
        ensure!(
            teleport_id == 1,
            "expected teleport id 1, got {teleport_id}"
        );

        // 30.Synchronize Player Position (Required, tells the client they're ready to spawn)
        self.send_packet(&play::PlayerPositionLookS2c {
            position: DVec3::new(0.0, 4.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            flags: PlayerPositionLookFlags::default(),
            teleport_id: 2.into(),
        })
        .await?;

        // 33. C → S: Client Command (sent either before or while receiving chunks, further testing
        //     needed, server handles correctly if not sent)
        // let pkt = self.recv_packet::<play::ClientCommandC2s>().await?;
        // info!("33. {pkt:#?}");

        // 34. S → C: inventory, entities, etc
        // todo

        // read packet
        loop {
            let frame = self.recv_packet_raw().await?;
            let id = frame.id;
            // hex
            info!("read packet id {id:#x}");

            // Set Player Position
            if id == play::PositionAndOnGroundC2s::ID {
                let pkt = frame.decode::<play::PositionAndOnGroundC2s>()?;
                info!("{pkt:#?}");
            }
        }
    }

    async fn server_status(mut self) -> anyhow::Result<()> {
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

async fn server() -> anyhow::Result<()> {
    // start socket 25565
    let listener = TcpListener::bind("0.0.0.0:25565").await?;

    let mut id = 0;

    // accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        let process = Io::new(stream);
        let action = process.server_process(id);
        let action = print_errors(action);

        tokio::spawn(action);
        id += 1;
    }
}

// async fn client() -> anyhow::Result<()> {
//     let stream = TcpStream::connect("localhost:25565").await?;
//     let process = Io::new(stream);
//     process.client_process().await?;
//     Ok(())
// }
//
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::Subscriber::builder().init();

    // client().await
    server().await
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
