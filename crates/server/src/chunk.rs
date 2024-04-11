//! Utilities for working with chunks.

use crate::bits::BitStorage;

/// Returns the minimum number of bits needed to represent the integer `n`.
pub const fn ceil_log2(x: u32) -> u32 {
    u32::BITS - x.leading_zeros()
}

/// Create a heightmap for the highest solid block at each position in the chunk.
pub fn heightmap(max_height: u32, current_height: u32) -> Vec<u64> {
    let bits = ceil_log2(max_height + 1);
    let mut data = BitStorage::new(bits as usize, 16 * 16, None).unwrap();

    for x in 0_usize..16 {
        for z in 0_usize..16 {
            let index = x + z * 16;
            data.set(index, u64::from(current_height) + 1);
        }
    }

    data.into_data()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ceil_log2() {
        assert_eq!(super::ceil_log2(0), 0);
        assert_eq!(super::ceil_log2(1), 1);
        assert_eq!(super::ceil_log2(2), 2);
        assert_eq!(super::ceil_log2(3), 2);
        assert_eq!(super::ceil_log2(4), 3);
        assert_eq!(super::ceil_log2(5), 3);
        assert_eq!(super::ceil_log2(6), 3);
        assert_eq!(super::ceil_log2(7), 3);
        assert_eq!(super::ceil_log2(8), 4);
        assert_eq!(super::ceil_log2(9), 4);
        assert_eq!(super::ceil_log2(10), 4);
        assert_eq!(super::ceil_log2(11), 4);
        assert_eq!(super::ceil_log2(12), 4);
        assert_eq!(super::ceil_log2(13), 4);
        assert_eq!(super::ceil_log2(14), 4);
        assert_eq!(super::ceil_log2(15), 4);
        assert_eq!(super::ceil_log2(16), 5);
        assert_eq!(super::ceil_log2(17), 5);
        assert_eq!(super::ceil_log2(18), 5);
    }
}
