use std::ops::Range;

use fnv::FnvHashMap;

use crate::utils::{
    cache::hilbert::HilbertCache,
    group::{group, RangeInclusive},
    pow2::bits_for_length,
};

pub mod utils;

type Idx = u32;

#[derive(Debug, Clone, Copy)]
struct Node {
    start: Idx,
}

pub struct World {
    grid: Box<[Node]>,
    data: Vec<u8>,
    order: u8,
    width: u16,
    cache: HilbertCache,
}

pub struct Coord {
    pub x: u16,
    pub y: u16,
}

impl World {
    /// # Errors
    /// Returns an error if the total number of elements given the order is too large to fit in a
    /// u16
    pub fn create(width: u16) -> anyhow::Result<Self> {
        const NODE: Node = Node { start: 0 };

        let width_bits = bits_for_length(width);
        let width = 1_usize << width_bits;

        let grid = vec![NODE; width * width + 1].into_boxed_slice();

        Ok(Self {
            grid,
            data: vec![],
            order: width_bits,

            // it is not possible to be greater than a u16
            #[allow(clippy::cast_possible_truncation)]
            width: width as u16,
            // cache: Cache::default(),
            cache: HilbertCache::build(width_bits)?,
        })
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // we know this is valid because we are initializing with a u16 width
    pub const fn area(&self) -> u32 {
        // because last node is dummy node
        self.grid.len() as u32 - 1
    }

    #[allow(clippy::cast_possible_truncation)] // we know this is valid because we are initializing with a u16 width
    pub fn populate<F>(&mut self, mut f: F)
    where
        F: FnMut(Coord, &mut Vec<u8>),
    {
        for i in 0..self.area() {
            let (x, y) = fast_hilbert::h2xy(i, self.order);
            f(Coord { x, y }, &mut self.data);
        }

        let Some(grid) = self.grid.last_mut() else {
            unreachable!("grid is always at least 1 element")
        };

        let Ok(len) = Idx::try_from(self.data.len()) else {
            unreachable!("data.len() is always less than or equal to u32::MAX")
        };

        grid.start = len;
    }

    #[must_use]
    pub fn idx(&self, x: u16, y: u16) -> u32 {
        debug_assert!(x < self.width);
        debug_assert!(y < self.width);
        self.cache.get_hilbert(x, y)
    }

    #[must_use]
    fn get_data_idx(&self, x: u16, y: u16) -> Range<usize> {
        let idx = self.idx(x, y);

        let start = self.grid[idx as usize].start;
        let stop = self.grid[idx as usize + 1].start;

        start as usize..stop as usize
    }

    #[must_use]
    pub fn get_data(&self, x: u16, y: u16) -> &[u8] {
        let range = self.get_data_idx(x, y);
        &self.data[range]
    }

    pub fn get_data_mut(&mut self, x: u16, y: u16) -> &mut [u8] {
        let range = self.get_data_idx(x, y);
        &mut self.data[range]
    }

    pub fn data_range(
        &mut self,
        x_range: Range<u16>,
        y_range: Range<u16>,
    ) -> impl Iterator<Item = &[u8]> {
        x_range
            .flat_map(move |x| y_range.clone().map(move |y| (x, y)))
            .map(|(x, y)| self.get_data(x, y))
    }
}
