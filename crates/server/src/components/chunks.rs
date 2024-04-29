use std::{borrow::Cow, collections::HashMap, io::Write};

use anyhow::Context;
use bytes::{Bytes, BytesMut};
use evenio::component::Component;
use fxhash::FxBuildHasher;
use itertools::Itertools;
use rayon_local::RayonLocal;
use valence_generated::block::BlockState;
use valence_nbt::{compound, List};
use valence_protocol::{packets::play, ChunkPos, Encode, FixedArray};
use valence_registry::{BiomeRegistry, RegistryIdx};
use valence_server::layer::chunk::{bit_width, BiomeContainer, BlockStateContainer, UnloadedChunk};

use crate::{bits::BitStorage, blocks::AnvilFolder, chunk::heightmap, net, net::Compose};

#[derive(Debug)]
pub struct LoadedChunk {
    // inner: UnloadedChunk,
    pub raw: Bytes,
}

#[derive(Debug, Component)]
pub struct Chunks {
    loader: parking_lot::Mutex<AnvilFolder>,

    bytes: RayonLocal<BytesMut>,

    // todo: impl more efficient (probably lru) cache
    cache: parking_lot::RwLock<HashMap<ChunkPos, LoadedChunk, FxBuildHasher>>,
}

impl Chunks {
    pub fn new(registry: &BiomeRegistry) -> anyhow::Result<Self> {
        let loader = AnvilFolder::new(registry).context("failed to get anvil data")?;
        Ok(Self {
            loader: loader.into(),
            bytes: RayonLocal::default(),
            cache: HashMap::with_hasher(FxBuildHasher::default()).into(),
        })
    }

    // todo: eliminate compose requirement
    // pub fn get_block(&self, pos: IVec3) -> anyhow::Result<Option<BlockState>> {
    //     let chunk_pos = ChunkPos::new(pos.x >> 4, pos.z >> 4);
    //     let cache = self.cache.read();
    //     let Some(chunk) = cache.get(&chunk_pos) else {
    //         return Ok(None);
    //     };
    //
    //     let chunk_start_pos = IVec3::new(chunk_pos.x << 4, -64, chunk_pos.z << 4);
    //     let relative_pos = (pos - chunk_start_pos).as_uvec3();
    //
    //     Ok(Some(chunk.inner.block_state(
    //         relative_pos.x,
    //         relative_pos.y,
    //         relative_pos.z,
    //     )))
    // }

    pub fn get(&self, pos: ChunkPos, compose: &Compose) -> anyhow::Result<Option<Bytes>> {
        {
            let cache = self.cache.read();
            if let Some(result) = cache.get(&pos) {
                return Ok(Some(result.raw.clone()));
            }
        }

        let chunk = {
            // todo: do we need {} ?
            // todo: could probably make loader more efficient for multi-threaded access
            self.loader.lock().dim.get_chunk(pos)
        };

        let Ok(chunk) = chunk else {
            return Ok(None);
        };

        let Some(chunk) = chunk else {
            return Ok(None);
        };

        let bufs = compose.bufs.get_local();
        let mut bufs = bufs.borrow_mut();
        let enc = bufs.enc_mut();

        let bytes = self.bytes.get_local_raw();
        let bytes = unsafe { &mut *bytes.get() };

        let chunk = chunk.chunk;

        let bytes_in_packet = encode_chunk_packet(&chunk, pos, bytes, compose, enc)?;

        let Some(bytes_in_packet) = bytes_in_packet else {
            return Ok(None);
        };

        let bytes_in_packet = bytes_in_packet.freeze();

        {
            // write
            let mut cache = self.cache.write();
            cache.insert(pos, LoadedChunk {
                // inner: chunk,
                raw: bytes_in_packet.clone(),
            });
        }

        Ok(Some(bytes_in_packet))
    }
}

fn encode_chunk_packet(
    chunk: &UnloadedChunk,
    location: ChunkPos,
    buf: &mut BytesMut,
    compose: &Compose,
    encoder: &net::encoder::PacketEncoder,
) -> anyhow::Result<Option<BytesMut>> {
    let section_count = 384 / 16_usize;
    let dimension_height = 384;

    let map = heightmap(dimension_height, dimension_height - 3);
    let map = map.into_iter().map(i64::try_from).try_collect()?;

    // convert section_count + 2 0b1s into u64 array
    let mut bits = BitStorage::new(1, section_count + 2, None).unwrap();

    for i in 0..section_count + 2 {
        bits.set(i, 1);
    }

    // 2048 bytes per section -> long count = 2048 / 8 = 256
    let sky_light_array = FixedArray([0xFF_u8; 2048]);
    let sky_light_arrays = vec![sky_light_array; section_count + 2];

    let mut section_bytes = Vec::new();

    for section in &chunk.sections {
        let non_air_blocks: u16 = 42;
        non_air_blocks.encode(&mut section_bytes).unwrap();

        write_block_states(&section.block_states, &mut section_bytes).unwrap();
        write_biomes(&section.biomes, &mut section_bytes).unwrap();
    }

    let pkt = play::ChunkDataS2c {
        pos: location,
        heightmaps: Cow::Owned(compound! {
            "MOTION_BLOCKING" => List::Long(map),
        }),
        blocks_and_biomes: &section_bytes,
        block_entities: Cow::Borrowed(&[]),

        sky_light_mask: Cow::Owned(bits.into_data()),
        block_light_mask: Cow::Borrowed(&[]),
        empty_sky_light_mask: Cow::Borrowed(&[]),
        empty_block_light_mask: Cow::Borrowed(&[]),
        sky_light_arrays: Cow::Owned(sky_light_arrays),
        block_light_arrays: Cow::Borrowed(&[]),
    };

    let mut scratch = compose.scratch.get_local().borrow_mut();
    let mut compressor = compose.compressor.get_local().borrow_mut();

    let scratch = &mut *scratch;
    let compressor = &mut *compressor;

    let result = encoder.append_packet(&pkt, buf, scratch, compressor)?;

    Ok(Some(result))
}

fn write_block_states(states: &BlockStateContainer, writer: &mut impl Write) -> anyhow::Result<()> {
    states.encode_mc_format(
        writer,
        |b| b.to_raw().into(),
        4,
        8,
        bit_width(BlockState::max_raw().into()),
    )?;
    Ok(())
}

fn write_biomes(biomes: &BiomeContainer, writer: &mut impl Write) -> anyhow::Result<()> {
    biomes.encode_mc_format(
        writer,
        |b| b.to_index() as u64,
        0,
        3,
        6, // bit_width(info.biome_registry_len - 1),
    )?;
    Ok(())
}
