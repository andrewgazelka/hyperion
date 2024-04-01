use anyhow::Context;

// todo: need to figure out how to name things well lololol
#[allow(clippy::module_name_repetitions)]
pub struct HilbertCache {
    xy_to_hilbert: Box<[u32]>,
    order: u8,
}

const fn xy_index(x: u16, y: u16, width: u16) -> usize {
    (x + y * width) as usize
}

impl HilbertCache {
    /// # Errors
    /// Returns an error if the total number of elements given the order is too large to fit in a
    /// u16
    pub fn build(order: u8) -> anyhow::Result<Self> {
        let len = 1 << (order * 2);

        let mut xy_to_hilbert = vec![0; len].into_boxed_slice();

        // todo: choose appropriate width int size
        let width = u16::try_from(1_usize << order)
            .context("The total number of elements given the order is too large to fit in a u16")?;

        for x in 0..width {
            for y in 0..width {
                xy_to_hilbert[xy_index(x, y, width)] = fast_hilbert::xy2h(x, y, order);
            }
        }

        Ok(Self {
            xy_to_hilbert,
            order,
        })
    }

    #[must_use]
    pub const fn get_hilbert(&self, x: u16, y: u16) -> u32 {
        self.xy_to_hilbert[xy_index(x, y, 1 << self.order)]
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::cache::hilbert::HilbertCache;

    #[test]
    fn test_order_1() {
        let cache = HilbertCache::build(1).unwrap();

        assert_eq!(cache.xy_to_hilbert.len(), 4);

        assert_eq!(cache.get_hilbert(0, 0), 0);
        assert_eq!(cache.get_hilbert(0, 1), 1);
        assert_eq!(cache.get_hilbert(1, 1), 2);
        assert_eq!(cache.get_hilbert(1, 0), 3);
    }

    #[test]
    fn test_order_2() {
        let cache = HilbertCache::build(2).unwrap();

        assert_eq!(cache.xy_to_hilbert.len(), 16);

        assert_eq!(cache.get_hilbert(0, 0), 0);
        assert_eq!(cache.get_hilbert(1, 0), 1);
        assert_eq!(cache.get_hilbert(1, 1), 2);
        assert_eq!(cache.get_hilbert(0, 1), 3);
        assert_eq!(cache.get_hilbert(0, 2), 4);
        assert_eq!(cache.get_hilbert(0, 3), 5);
        assert_eq!(cache.get_hilbert(1, 3), 6);
        assert_eq!(cache.get_hilbert(1, 2), 7);
        assert_eq!(cache.get_hilbert(2, 2), 8);
        assert_eq!(cache.get_hilbert(2, 3), 9);
        assert_eq!(cache.get_hilbert(3, 3), 10);
        assert_eq!(cache.get_hilbert(3, 2), 11);
        assert_eq!(cache.get_hilbert(3, 1), 12);
        assert_eq!(cache.get_hilbert(2, 1), 13);
        assert_eq!(cache.get_hilbert(2, 0), 14);
        assert_eq!(cache.get_hilbert(3, 0), 15);
    }
}
