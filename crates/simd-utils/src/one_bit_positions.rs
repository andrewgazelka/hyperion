use std::iter::{FusedIterator, TrustedLen};

pub struct OneBitPositions {
    pub remaining: u64,
}

impl OneBitPositions {
    const fn new(number: u64) -> Self {
        Self { remaining: number }
    }
}

impl Iterator for OneBitPositions {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            // Get position of lowest set bit
            let pos = self.remaining.trailing_zeros();
            // Clear the lowest set bit
            self.remaining &= self.remaining - 1;
            Some(pos as usize)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for OneBitPositions {
    fn len(&self) -> usize {
        self.remaining.count_ones() as usize
    }
}

impl FusedIterator for OneBitPositions {}

unsafe impl TrustedLen for OneBitPositions {}

// Extension trait for more ergonomic usage
pub trait OneBitPositionsExt {
    fn one_positions(self) -> OneBitPositions;
}

impl OneBitPositionsExt for u64 {
    fn one_positions(self) -> OneBitPositions {
        OneBitPositions::new(self)
    }
}
