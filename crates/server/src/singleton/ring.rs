use std::io::Write;

use libc::iovec;
use tracing::{debug, info};

use crate::net::ServerDef;

// todo: see if it makes sense to use MaybeUninit
#[derive(Debug)]
pub struct Ring {
    data: Box<[u8]>, // sad we have to box this so no stackoverflow
    head: usize,
    max_len: usize,
}

pub trait McBuf {
    fn len_until_end(&self) -> usize;
    fn get_contiguous(&mut self, len: usize) -> &mut [u8];
    fn advance(&mut self, len: usize);

    /// Returns a pointer to the first byte of the appended data.
    fn append(&mut self, data: &[u8]) -> *const u8;
}

impl McBuf for Ring {
    fn len_until_end(&self) -> usize {
        self.max_len - self.head
    }

    fn get_contiguous(&mut self, len: usize) -> &mut [u8] {
        debug_assert!(len <= self.max_len);

        let len_until_end = self.len_until_end();
        if len_until_end < len {
            info!("rotating buffer because {len_until_end} < {len}");
            self.head = 0;
            &mut self.data[..len]
        } else {
            let start = self.head;
            &mut self.data[start..start + len]
        }
    }

    /// **Does not advice head unless it needs to move to the beginning**
    fn advance(&mut self, len: usize) {
        debug_assert!(len <= self.max_len);
        self.head = (self.head + len) % self.max_len;
    }

    fn append(&mut self, data: &[u8]) -> *const u8 {
        debug_assert!(data.len() <= self.max_len);
        let len = data.len();
        let contiguous = self.get_contiguous(len);
        contiguous.copy_from_slice(data);
        let ptr = contiguous.as_ptr();
        self.advance(len);
        ptr
    }
}

impl Ring {
    pub fn new(max_len: usize) -> Self {
        Self {
            data: vec![0; max_len].into_boxed_slice(),
            head: 0,
            max_len,
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
