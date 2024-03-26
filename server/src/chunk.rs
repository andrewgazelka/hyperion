// Compound containing one long array named MOTION_BLOCKING, which is a heightmap for the highest
// solid block at each position in the chunk (as a compacted long array with 256 entries, with the
// number of bits per entry varying depending on the world's height, defined by the formula
// ceil(log2(height + 1))). The Notchian server also adds a WORLD_SURFACE long array, the purpose of
// which is unknown, but it's not required for the chunk to be accepted.

use crate::bits::BitStorage;

pub const fn ceil_log2(x: u32) -> u32 {
    u32::BITS - x.leading_zeros()
}

pub fn heightmap(max_height: u32, current_height: u32) -> Vec<u64> {
    let bits = ceil_log2(max_height + 1);
    let mut data = BitStorage::new(bits as usize, 16 * 16, None).unwrap();

    for x in 0_usize..16 {
        for z in 0_usize..16 {
            let index = x + z * 16;
            data.set(index, current_height as u64 + 1);
        }
    }

    data.into_data()
}
