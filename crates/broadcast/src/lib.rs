//! A crate for efficiently broadcasting data, supporting generic types.
//!
//! The `broadcast` crate provides functionality for broadcasting data, commonly slices of `u8`,
//! but it is generic and supports any type. It's particularly useful for applications that need to
//! serialize or encode data packets and query byte slices based on certain criteria, such as the
//! location of a player in a grid.
//!
//! # Efficiency
//! The crate stores all bytes in a single `Vec`, allowing for efficient access and transmission
//! of multiple byte chunks to, for example, a player, at one time.
//!
//! # Examples
//!
//! Basic usage for broadcasting slices of `u8`:
//!
//! ```rust
//! use broadcast::Broadcaster;
//!
//! fn main() -> anyhow::Result<()> {
//!     let mut broadcaster = Broadcaster::<u8>::create(4)?;
//!
//!     // Populate the broadcaster with some data.
//!     broadcaster.repopulate(|coord, data| {
//!         // For simplicity, just push the sum of x and y coordinates as data.
//!         data.push((coord.x + coord.y) as u8);
//!         data.push((coord.x * coord.y) as u8);
//!     });
//!
//!     // Retrieve data for a specific grid coordinate.
//!     let x = 1;
//!     let y = 2;
//!     let data = broadcaster.get_data(x, y);
//!     assert_eq!(data, &[3_u8, 2_u8]);
//!
//!     // Retrieve and mutate data for a specific grid coordinate.
//!     let data_mut = broadcaster.get_data_mut(x, y);
//!     data_mut[0] = 10; // Example mutation.
//!     assert_eq!(data_mut, &[10_u8, 2_u8]);
//!
//!     // Retrieve data for a range of grid coordinates.
//!     let x_range = 0..2;
//!     let y_range = 1..3;
//!
//!     // tries to use ranges whenever possible
//!     for data_slice in broadcaster.data_range(x_range, y_range) {
//!         println!("{:?}", data_slice);
//!     }
//!
//!     Ok(())
//! }
//! ```

use std::ops::Range;

pub use glam::U16Vec2;

type Idx = u32;

#[derive(Debug, Clone, Copy)]
struct Node {
    start: Idx,
}

/// A generic, efficient data broadcasting structure.
///
/// `Broadcaster` allows for storing and efficiently querying data associated with coordinates
/// on a grid. The structure is generic over the type of data it holds, making it versatile for
/// various applications, especially where spatial querying and data serialization are required.
///
/// # Generics
/// - `T`: The type of the elements stored. Commonly used with `u8` for byte slices, but can be
/// any type.
pub struct Broadcaster<T = u8> {
    grid: Box<[Node]>,
    data: Vec<T>,
    width: u16,
}

impl<T> Broadcaster<T> {
    /// # Errors
    /// Returns an error if the total number of elements given the order is too large to fit in a
    /// u16
    pub fn create(width: u16) -> anyhow::Result<Self> {
        const NODE: Node = Node { start: 0 };

        let area = u32::from(width) * u32::from(width);

        // +1 because we need a dummy node at the end
        let grid = vec![NODE; (area + 1) as usize].into_boxed_slice();

        Ok(Self {
            grid,
            data: vec![],
            width,
        })
    }

    /// Returns the area of the grid.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // we know this is valid because we are initializing with a u16 width
    pub const fn area(&self) -> u32 {
        // because last node is dummy node
        self.grid.len() as u32 - 1
    }

    /// Populates the broadcaster with data using a provided function.
    ///
    /// This method clears any existing data and repopulates the `Broadcaster`'s data vector based
    /// on the function provided. It iterates over the entire grid, converting each index to its
    /// corresponding (x, y) coordinate, and then calls the provided function with these coordinates
    /// and a mutable reference to the data vector.
    ///
    /// # Type Parameters
    /// - `F`: A closure or function pointer that takes a `U16Vec2` representing the (x, y)
    ///   coordinate and a mutable reference to a `Vec<T>`, where `T` is the type of data stored in
    ///   the broadcaster.
    ///
    /// # Arguments
    /// - `f`: The function to be called for each (x, y) coordinate in the grid. This function is
    ///   responsible for populating the broadcaster's data vector.
    ///
    /// # Behavior
    /// - The method first clears the broadcaster's data vector to remove any previously stored
    ///   data.
    /// - It then iterates over every point in the grid, calculates its (x, y) coordinates, and
    ///   invokes the provided function `f` with these coordinates and a mutable reference to the
    ///   data vector.
    /// - After populating the data, it sets the start index of the last node in the grid to
    ///   represent the end of the data vector, ensuring the grid accurately reflects the data's
    ///   spatial distribution.
    ///
    /// # Panics
    /// - This method panics if it's unable to set the start index of the last node in the grid,
    ///   which should be unreachable under normal circumstances since the data length should always
    ///   be within the range representable by an `Idx` (u32).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use broadcast::Broadcaster;
    /// # use broadcast::U16Vec2;
    /// let mut broadcaster = Broadcaster::<u8>::create(4).unwrap();
    ///
    /// broadcaster.repopulate(|coord, data| {
    ///     // Example: Populate with the sum of the x and y coordinates.
    ///     data.push((coord.x + coord.y) as u8);
    ///     data.push((coord.x * coord.y) as u8);
    /// });
    /// ```
    ///
    /// This method is especially useful for initializing or updating the broadcaster's data in
    /// response to changes in the grid or the data itself.
    #[allow(clippy::cast_possible_truncation)] // we know this is valid because we are initializing with a u16 width
    pub fn repopulate<F>(&mut self, mut f: F)
    where
        F: FnMut(U16Vec2, &mut Vec<T>),
    {
        self.data.clear();
        for i in 0..self.area() {
            let (x, y) = self.idx_to_xy(i);
            let coord = U16Vec2::new(x, y);
            f(coord, &mut self.data);
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
    const fn xy_to_idx(&self, x: u16, y: u16) -> u32 {
        debug_assert!(x < self.width);
        debug_assert!(y < self.width);
        let x = x as u32;
        let y = y as u32;
        x + y * (self.width as u32)
    }

    #[must_use]
    const fn idx_to_xy(&self, idx: u32) -> (u16, u16) {
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

    /// Returns a reference to the data at the given grid coordinates.
    #[must_use]
    pub fn get_data(&self, x: u16, y: u16) -> &[T] {
        let range = self.get_data_idx(x, y);
        &self.data[range]
    }

    /// Returns a mutable reference to the data at the given grid coordinates.
    pub fn get_data_mut(&mut self, x: u16, y: u16) -> &mut [T] {
        let range = self.get_data_idx(x, y);
        &mut self.data[range]
    }

    /// Returns an iterator over data slices within specified ranges of x and y coordinates.
    ///
    /// This method is designed to efficiently retrieve data slices that are contiguous in memory,
    /// minimizing the number of slices and combining them wherever possible. It is particularly
    /// useful for applications requiring batch access to spatial data, such as rendering or
    /// processing regions of a grid.
    pub fn data_range(
        &self,
        x_range: Range<u16>,
        y_range: Range<u16>,
    ) -> impl Iterator<Item = &[T]> + '_ {
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
