use std::ops::Range;

use crate::utils::pow2::bits_for_length;

pub mod utils;

type Idx = u32;

#[derive(Debug, Clone, Copy)]
struct Node {
    start: Idx,
}

pub struct Broadcaster {
    grid: Box<[Node]>,
    data: Vec<u8>,
    width: u16,
}

pub struct Coord {
    pub x: u16,
    pub y: u16,
}

impl Broadcaster {
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

            // it is not possible to be greater than a u16
            #[allow(clippy::cast_possible_truncation)]
            width: width as u16,
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
            let (x, y) = self.idx_to_xy(i);
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
    pub const fn xy_to_idx(&self, x: u16, y: u16) -> u32 {
        debug_assert!(x < self.width);
        debug_assert!(y < self.width);
        let x = x as u32;
        let y = y as u32;
        x + y * (self.width as u32)
    }

    #[must_use]
    pub const fn idx_to_xy(&self, idx: u32) -> (u16, u16) {
        let x = idx % self.width as u32;
        let y = idx / self.width as u32;
        #[allow(clippy::cast_possible_truncation)]
        (x as u16, y as u16)
    }

    #[must_use]
    const fn get_data_idx(&self, x: u16, y: u16) -> Range<usize> {
        let idx = self.xy_to_idx(x, y);

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
        &self,
        x_range: Range<u16>,
        y_range: Range<u16>,
    ) -> impl Iterator<Item = &[u8]> + '_ {
        let x_start = x_range.start;
        let x_end = x_range.end;

        y_range
            .map(move |y| {
                let start_idx = self.xy_to_idx(x_start, y);
                let stop_idx = self.xy_to_idx(x_end, y) + 1;

                let data_start_idx = self.grid[start_idx as usize].start as usize;
                let data_stop_idx = self.grid[stop_idx as usize].start as usize;

                data_start_idx..data_stop_idx
            })
            .map(move |range| &self.data[range])
    }
}
