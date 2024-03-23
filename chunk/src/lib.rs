#![allow(unused)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::missing_panics_doc)]
// https://wiki.vg/Chunk_Format#Data_structure
// https://wiki.vg/index.php?title=Chunk_Format&oldid=18480

use std::io::Write;

use byteorder::{BigEndian, WriteBytesExt};
use valence_protocol::Encode;

pub mod chunk;
pub mod paletted_container;

/// Returns the minimum number of bits needed to represent the integer `n`.
#[must_use]
pub const fn bit_width(n: usize) -> usize {
    (usize::BITS - n.leading_zeros()) as usize
}
