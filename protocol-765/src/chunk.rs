#![allow(unused)]
#![allow(clippy::unused_self)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::module_name_repetitions)]
/// the backing of sections either an array of size N or a Vec
trait Backing {}

// overworld is 384 blocks high so 24 sections
/// <https://wiki.vg/Chunk_Format#Data_structure>
pub struct ChunkData {
    pub sections: Vec<ChunkSection>,
}

impl Writable for ChunkData {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        for section in &self.sections {
            section.write(writer)?;
        }
        Ok(())
    }
}

impl ChunkData {
    pub(crate) fn new() -> Self {
        Self {
            sections: vec![ChunkSection::default(); 24],
        }
    }
}

/// <https://wiki.vg/Chunk_Format#Chunk_Section_structure>
#[derive(Copy, Clone, Default)]
pub struct ChunkSection {
    // /// Number of non-air blocks present in the chunk section. "Non-air" is defined as any fluid
    // /// and block other than air, cave air, and void air. The client will keep count of the
    // blocks /// as they are broken and placed, and, if the block count reaches 0, the whole
    // chunk section /// is not rendered, even if it still has blocks.
    // pub block_count: u16,
    //
    // /// Consists of 4096 entries, representing all the blocks in the chunk section.
    // pub palette: PalettedContainer,
    //
    // /// Consists of 64 entries, representing 4×4×4 biome regions in the chunk section.
    // // pub biomes: [u8; 64],
    // pub biomes: BiomePalette<'a>,
    pub builder: ChunkSectionBuilder,
}

impl From<ChunkSectionBuilder> for ChunkSection {
    fn from(builder: ChunkSectionBuilder) -> Self {
        Self { builder }
    }
}

impl Writable for ChunkSection {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        write_chunk_section(&self.builder, writer, false);
        BiomePalette::ALL_REGULAR.write(writer)
    }
}

struct BiomePalette<'a> {
    pub bits_per_block: u8,
    pub data: &'a [u64],
}

impl<'a> Writable for BiomePalette<'a> {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        self.bits_per_block.write(writer);
        let data = bytemuck::cast_slice(self.data);
        writer.write_all(data)?;
        Ok(())
    }
}

impl BiomePalette<'static> {
    const ALL_REGULAR: Self = Self {
        bits_per_block: 0,
        // main biome id for PLAINS
        data: &[1],
    };
}

/// 16×16×16 blocks
#[derive(Copy, Clone)]
pub struct ChunkSectionBuilder {
    states: [BlockState; 16 * 16 * 16],
}

impl Default for ChunkSectionBuilder {
    fn default() -> Self {
        Self {
            states: [BlockState::default(); 16 * 16 * 16],
        }
    }
}

impl ChunkSectionBuilder {
    #[must_use]
    pub fn block_count(&self) -> u16 {
        // all that are not air TODO: include void air, etc
        self.states.iter().filter(|state| state.id() != 0).count() as u16
    }

    #[must_use]
    pub fn fill(state: BlockState) -> Self {
        let mut builder = Self::default();

        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    builder.set_state(x, y, z, state);
                }
            }
        }

        builder
    }

    fn get_state(&self, x: usize, y: usize, z: usize) -> BlockState {
        self.states[(y * 16 * 16) + (z * 16) + x]
    }

    fn set_state(&mut self, x: usize, y: usize, z: usize, state: BlockState) {
        self.states[(y * 16 * 16) + (z * 16) + x] = state;
    }
}

/// 4096 entries
struct PalettedContainer {
    pub bits_per_block: u8,
    pub data: Vec<u64>,
}

impl PalettedContainer {
    const fn get_bits_per_block(&self) -> u8 {
        self.bits_per_block
    }

    const fn id_for_state(&self, state: BlockState) -> u64 {
        0
    }
}

/// `https://wiki.vg/Chunk_Format#Palette_formats`
enum DirectPalette {}

use std::io::Write;

use byteorder::{BigEndian, WriteBytesExt};
use bytes::{BufMut, BytesMut};
use ser::{types::VarUInt, Writable};

struct Palette {
    // Palette implementation
}

impl Palette {
    fn get_bits_per_block(&self) -> u8 {
        // Dummy implementation
        0
    }

    fn id_for_state(&self, _state: BlockState) -> u64 {
        // Dummy implementation
        0
    }

    fn write(&self, _buf: &mut impl Write) {
        // Write palette data to buffer
    }
}

#[derive(Copy, Clone, Default)]
pub struct BlockState {
    id: u64,
}

impl BlockState {
    // minecraft id for DIRT
    pub const DIRT: Self = Self { id: 3 };

    fn id(self) -> u64 {
        // Dummy implementation
        self.id
    }
}

#[allow(clippy::cognitive_complexity)]
fn write_chunk_section(
    section: &ChunkSectionBuilder,
    buf: &mut impl std::io::Write,
    current_dimension_has_skylight: bool,
) -> anyhow::Result<()> {
    let count = section.block_count();
    buf.write_u16::<BigEndian>(count)?;

    // let palette = &section.palette;
    // let bits_per_block = palette.get_bits_per_block();
    let bits_per_block = 15;

    buf.write_u8(bits_per_block);

    let data_length = (16 * 16 * 16 * bits_per_block as usize + 63) / 64;

    VarUInt(data_length as u32).write(buf);

    let bytes_needed = data_length * 8;

    let mut data = vec![0; data_length];

    let individual_value_mask = (1u64 << bits_per_block) - 1;

    for y in 0..SECTION_HEIGHT {
        for z in 0..SECTION_WIDTH {
            for x in 0..SECTION_WIDTH {
                let block_number = ((y * SECTION_HEIGHT + z) * SECTION_WIDTH) + x;
                let start_long = (block_number * bits_per_block as usize) / 64;
                let start_offset = (block_number * bits_per_block as usize) % 64;
                let end_long = ((block_number + 1) * bits_per_block as usize - 1) / 64;

                let state = section.get_state(x, y, z);

                // let mut value = palette.id_for_state(state);
                let mut value = state.id();

                value &= individual_value_mask;

                data[start_long] |= value << start_offset;

                if start_long != end_long {
                    data[end_long] |= value >> (64 - start_offset);
                }
            }
        }
    }

    buf.write_all(bytemuck::cast_slice(&data));

    for y in 0..SECTION_HEIGHT {
        // 16
        for z in 0..SECTION_WIDTH {
            // 16
            for x in (0..SECTION_WIDTH).step_by(2) {
                // 8
                // let value =
                //     section.get_block_light(x, y, z) | (section.get_block_light(x + 1, y, z) <<
                // 4);
                let value = 0;
                buf.write_u8(value)?;
            }
        }
    }

    if current_dimension_has_skylight {
        for y in 0..SECTION_HEIGHT {
            for z in 0..SECTION_WIDTH {
                for x in (0..SECTION_WIDTH).step_by(2) {
                    // let value =
                    //     section.get_sky_light(x, y, z) | (section.get_sky_light(x + 1, y, z) <<
                    // 4);
                    let value = 0;
                    buf.write_u8(value)?;
                }
            }
        }
    }

    Ok(())
}

const SECTION_HEIGHT: usize = 16;
const SECTION_WIDTH: usize = 16;
