pub mod utils;

use std::{collections::HashMap, ops::Range};

use fnv::FnvHashMap;

use crate::utils::{
    group::{group, RangeInclusive},
    pow2::{bits_for_length, closest_consuming_power_of_2},
};

type Idx = u32;

#[derive(Debug, Clone, Copy)]
struct Node {
    start: Idx,
}

#[derive(Default)]
struct Cache {
    inner: FnvHashMap<(u16, u16), Vec<RangeInclusive>>,
}

impl Cache {
    fn calculate(
        &mut self,
        order: u8,
        x_range: Range<u16>,
        y_range: Range<u16>,
    ) -> &mut Vec<RangeInclusive> {
        self.inner
            // yes I know we are not caching the whole range so this is technically incorrect
            // but let's just assume that range lengths are always the same
            .entry((x_range.start, y_range.start))
            .or_insert_with(|| {
                let mut indices = Vec::with_capacity(x_range.len() * y_range.len());

                for x in x_range {
                    for y in y_range.clone() {
                        indices.push(fast_hilbert::xy2h(x, y, order));
                    }
                }

                indices.sort_unstable();

                group(indices).collect()
            })
    }
}

pub struct World {
    grid: Box<[Node]>,
    data: Vec<u8>,
    order: u8,
    width: u16,
    cache: Cache,
}

pub struct Coord {
    pub x: u16,
    pub y: u16,
}

impl World {
    #[must_use]
    pub fn create(width: u16) -> Self {
        const NODE: Node = Node { start: 0 };

        // let width_bits = 7; // closest_consuming_power_of_2(width - 1);
        let width_bits = bits_for_length(width);
        let width = 1_usize << width_bits;

        let grid = vec![NODE; width * width + 1].into_boxed_slice();

        Self {
            grid,
            data: vec![],
            order: width_bits,

            // it is not possible to be greater than a u16
            #[allow(clippy::cast_possible_truncation)]
            width: width as u16,
            cache: Cache::default(),
        }
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
        fast_hilbert::xy2h(x, y, self.order)
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

    pub fn data_range(&mut self, x_range: Range<u16>, y_range: Range<u16>) -> Vec<&[u8]> {
        let indices = self.cache.calculate(self.order, x_range, y_range);

        indices
            .iter()
            .map(|idx| {
                let start = idx.start;
                let end = idx.end;

                let start = self.grid[start as usize].start as usize;
                let end = self.grid[end as usize + 1].start as usize;

                &self.data[start..end]
            })
            .collect()
    }
}
