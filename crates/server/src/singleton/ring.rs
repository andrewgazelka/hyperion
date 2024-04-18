use std::io::Write;

use libc::iovec;

use crate::net::ServerDef;

// todo: see if it makes sense to use MaybeUninit
#[derive(Debug)]
pub struct Ring<const N: usize> {
    data: [u8; N],
    head: usize,
}

pub trait McBuf {
    fn len_until_end(&self) -> usize;
    fn get_contiguous(&mut self, len: usize) -> &mut [u8];
    fn advance(&mut self, len: usize);
    fn append(&mut self, data: &[u8]);
}

impl<const N: usize> McBuf for Ring<N> {
    fn len_until_end(&self) -> usize {
        N - self.head
    }

    fn get_contiguous(&mut self, len: usize) -> &mut [u8] {
        debug_assert!(len <= N);

        if self.len_until_end() < len {
            self.head = 0;
            &mut self.data[..len]
        } else {
            let start = self.head;
            &mut self.data[start..self.head]
        }
    }

    /// **Does not advice head unless it needs to move to the beginning**
    fn advance(&mut self, len: usize) {
        debug_assert!(len <= N);
        self.head = (self.head + len) % N;
    }

    fn append(&mut self, data: &[u8]) {
        debug_assert!(data.len() <= N);
        let len = data.len();
        self.get_contiguous(len).copy_from_slice(data);
        self.advance(len);
    }
}

impl<const N: usize> Ring<N> {
    pub fn new() -> Self {
        Self {
            data: [0; N],
            head: 0,
        }
    }

    pub fn register(&mut self, server_def: &mut impl ServerDef) {
        let ptr = self.data.as_mut_ptr();
        let len = self.data.len();

        let to_register = iovec {
            iov_base: ptr.cast(),
            iov_len: len,
        };

        server_def.allocate_buffers(&[to_register]);
    }
}

// tests
#[cfg(test)]
mod tests {
    use std::mem::MaybeUninit;

    use super::*;
    #[test]
    fn test_new() {
        let ring = Ring::<10>::new();
        assert_eq!(ring.head, 0);
        assert_eq!(ring.len_until_end(), 10);

        let ring = Ring::<0>::new();
        assert_eq!(ring.head, 0);
        assert_eq!(ring.len_until_end(), 0);
    }
}
