// struct PalettedContainer {
//     bits_per_entry: u8,
// }

use std::io::Write;

use byteorder::WriteBytesExt;
use valence_protocol::{Encode, VarInt};

pub trait BlockGetter {
    fn get_state(&self, x: usize, y: usize, z: usize) -> u64;
}

pub struct DirectEncoding<B> {
    section: B,
}

// From
impl<B: BlockGetter> From<B> for DirectEncoding<B> {
    fn from(section: B) -> Self {
        Self { section }
    }
}

// impl<B: BlockGetter> Encode for DirectEncoding<B> {
//     #[allow(clippy::indexing_slicing)]
//     fn encode(&self, mut writer: impl Write) -> anyhow::Result<()> {
//         // 0 bits per block
//         writer.write_u8(0)?;
//
//         VarInt(2).encode(&mut writer)?; // 2-block
//         VarInt(0).encode(&mut writer)?; // empty array
//
//
//
//         // also encode single biome
//
//         // 0 bits per block
//         writer.write_u8(0)?; // single biome
//
//         VarInt(0).encode(&mut writer)?; // 0 biome
//         VarInt(0).encode(&mut writer)?; // empty array
//
//         Ok(())
//     }
// }

impl<B: BlockGetter> Encode for DirectEncoding<B> {
    #[allow(clippy::indexing_slicing)]
    fn encode(&self, mut writer: impl Write) -> anyhow::Result<()> {
        const BITS_PER_BLOCK: usize = 15;
        writer.write_u8(BITS_PER_BLOCK as u8)?;

        const SECTION_HEIGHT: usize = 16; // example dimensions
        const SECTION_WIDTH: usize = 16;

        let individual_value_mask: u64 = (1 << BITS_PER_BLOCK) - 1;

        let data_length = (16 * 16 * 16 * BITS_PER_BLOCK) / 64;
        let mut data: Vec<u64> = vec![0; data_length];

        #[allow(clippy::excessive_nesting)]
        for y in 0..SECTION_HEIGHT {
            for z in 0..SECTION_WIDTH {
                for x in 0..SECTION_WIDTH {
                    let block_number = (((y * SECTION_HEIGHT) + z) * SECTION_WIDTH) + x;
                    let start_long = (block_number * BITS_PER_BLOCK) / 64;
                    let start_offset = (block_number * BITS_PER_BLOCK) % 64;
                    let end_long = ((block_number + 1) * BITS_PER_BLOCK - 1) / 64;

                    // Assuming you have a way to get a block state from your section
                    // let state = self.section.get_state(x, y, z);
                    let state = 2; // example state
                    let mut value = state;

                    // let mut value = palette.id_for_state(&state); // Adjust for actual method
                    // call
                    value &= individual_value_mask;

                    data[start_long] |= value << start_offset;

                    if start_long != end_long {
                        data[end_long] = value >> (64 - start_offset);
                    }
                }
            }
        }

        data.encode(&mut writer)?;

        // also encode single biome

        // 0 bits per block
        writer.write_u8(0)?; // single biome

        VarInt(0).encode(&mut writer)?; // 0 biome
        VarInt(0).encode(&mut writer)?; // empty array

        Ok(())
    }
}
