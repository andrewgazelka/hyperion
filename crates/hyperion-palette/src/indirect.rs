use std::simd::{cmp::SimdPartialEq, Simd};

use crate::{Data, HALF_LEN, LEN};

pub struct Indirect {
    palette: [Data; 16],
    palette_len: u8,
    data: [u8; HALF_LEN],
}

struct Full;

impl Indirect {
    pub fn index_of(&self, data: Data) -> Option<u8> {
        // Create a SIMD vector filled with the search element
        let search_simd: Simd<u16, 16> = Simd::splat(data);

        // todo: is this zero cost?
        let chunk_simd: Simd<Data, 16> = Simd::from_array(self.palette);

        // Compare the chunk with the search element
        let mask = chunk_simd.simd_eq(search_simd);

        mask.first_set().map(|idx| idx as u8)
    }

    pub unsafe fn get_unchecked(&self, index: usize) -> Data {
        debug_assert!(index < LEN);

        let packed_byte_index = index / 2;
        let packed_byte = self.data[packed_byte_index];

        // Create a mask based on whether the index is even or odd
        // Either 0 or 4
        let shift = (index & 1) << 2;

        // Shift and mask to get the correct nibble
        let palette_index = (packed_byte >> shift) & 0x0F;

        self.palette[palette_index as usize]
    }

    pub fn get(&self, index: usize) -> Option<Data> {
        if index < LEN {
            Some(unsafe { self.get_unchecked(index) })
        } else {
            None
        }
    }

    pub unsafe fn set_unchecked(&mut self, index: usize, value: Data) -> Result<(), Full> {
        debug_assert!(index < LEN);

        let palette_index = match self.index_of(value) {
            Some(idx) => idx,
            None => {
                if self.palette_len == 16 {
                    return Err(Full);
                };
                let new_index = self.palette_len;
                self.palette[new_index] = value;
                self.palette_len += 1;
                new_index
            }
        };

        let packed_byte_index = index / 2;
        let shift = (index & 1) << 2;
        let mask = 0xF << shift;
        self.data[packed_byte_index] =
            (self.data[packed_byte_index] & !mask) | ((palette_index) << shift);
    }
}
