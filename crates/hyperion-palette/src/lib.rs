#![feature(portable_simd)]

use std::clone::Clone;

use crate::indirect::Indirect;

const LEN: usize = 4096;
const HALF_LEN: usize = LEN >> 1;

mod indirect;

mod encode;

type Data = u16;

#[derive(Clone, Debug)]
pub enum PalettedContainer {
    Single(Data),
    Indirect(Indirect),
    Direct(Box<[Data]>),
}

impl PalettedContainer {
    pub fn fill(&mut self, value: Data) {
        *self = Self::Single(value);
    }

    #[allow(clippy::missing_safety_doc)]
    #[must_use]
    pub unsafe fn get_unchecked(&self, index: usize) -> Data {
        match self {
            Self::Single(data) => *data,
            Self::Indirect(indirect) => unsafe { indirect.get_unchecked(index) },
            Self::Direct(direct) => unsafe { *direct.get_unchecked(index) },
        }
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Data {
        assert!(index < LEN);
        unsafe { self.get_unchecked(index) }
    }

    /// Returns the previous value
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn set_unchecked(&mut self, index: usize, value: Data) -> Data {
        match self {
            Self::Single(previous) => {
                if *previous == value {
                    return *previous;
                }

                let mut indirect = Indirect::from_single(*previous);
                let _ = unsafe { indirect.set_unchecked(index, value) }; // error can never occur
                let previous = *previous;
                *self = Self::Indirect(indirect);
                previous
            }
            Self::Indirect(indirect) => match unsafe { indirect.set_unchecked(index, value) } {
                Ok(previous) => previous,
                Err(indirect::Full) => {
                    let mut direct = indirect.to_direct();
                    let ptr = unsafe { direct.get_unchecked_mut(index) };
                    let previous = *ptr;
                    *ptr = value;
                    *self = Self::Direct(direct);
                    previous
                }
            },
            Self::Direct(direct) => {
                let ptr = unsafe { direct.get_unchecked_mut(index) };
                let previous = *ptr;
                *ptr = value;
                previous
            }
        }
    }
}
