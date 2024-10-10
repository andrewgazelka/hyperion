#![feature(portable_simd)]

use crate::indirect::Indirect;

const LEN: usize = 4096;
const HALF_LEN: usize = LEN >> 1;

mod indirect;

type Data = u16;


enum PalettedContainer {
    Single(Data),
    Indirect(Indirect),
}

impl 

struct Direct {
    data: [u8; LEN],
}