use std::mem::MaybeUninit;

use libc::iovec;
use tracing::warn;

use crate::net::{encoder::PacketWriteInfo, ServerDef};

// todo: see if it makes sense to use MaybeUninit
#[derive(Debug)]
pub struct Ring {
    data: Box<[u8]>, // sad we have to box this so no stackoverflow
    head: usize,
    max_len: usize,
}

pub trait Buf {
    type Output;
    fn get_contiguous(&mut self, len: usize) -> &mut [u8];
    fn advance(&mut self, len: usize) -> Self::Output;
}

impl Buf for bytes::BytesMut {
    type Output = Self;

    fn get_contiguous(&mut self, len: usize) -> &mut [u8] {
        // self.resize(len, 0);
        // self
        self.reserve(len);
        let cap = self.spare_capacity_mut();
        let cap = unsafe { MaybeUninit::slice_assume_init_mut(cap) };
        cap
    }

    fn advance(&mut self, len: usize) -> Self::Output {
        unsafe { self.set_len(self.len() + len) };
        self.split_to(len)
    }
}

impl Ring {
    const fn len_until_end(&self) -> usize {
        self.max_len - self.head
    }

    pub fn append(&mut self, data: &[u8]) -> *const u8 {
        debug_assert!(data.len() <= self.max_len);
        let len = data.len();
        let contiguous = self.get_contiguous(len);
        contiguous.copy_from_slice(data);
        let ptr = contiguous.as_ptr();
        self.advance(len);
        ptr
    }
}

impl Buf for Ring {
    type Output = PacketWriteInfo;

    fn get_contiguous(&mut self, len: usize) -> &mut [u8] {
        debug_assert!(
            len <= self.max_len,
            "requested contiguous length of {} exceeds max_len of {}",
            len,
            self.max_len
        );

        let len_until_end = self.len_until_end();
        if len_until_end < len {
            warn!("rotating buffer because {len_until_end} < {len}");
            self.head = 0;
            &mut self.data[..len]
        } else {
            let start = self.head;
            &mut self.data[start..start + len]
        }
    }

    /// **Does not advice head unless it needs to move to the beginning**
    fn advance(&mut self, len: usize) -> Self::Output {
        debug_assert!(len <= self.max_len);

        let start_ptr = unsafe { self.data.as_ptr().add(self.head) };

        self.head = (self.head + len) % self.max_len;

        let len = len as u32;
        PacketWriteInfo { start_ptr, len }
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

    fn as_iovec(&mut self) -> iovec {
        iovec {
            iov_base: self.data.as_mut_ptr().cast(),
            iov_len: self.data.len(),
        }
    }
}

pub fn register_rings<'a>(
    server_def: &mut impl ServerDef,
    io_buf: impl Iterator<Item = &'a mut Ring>,
) {
    let vec = io_buf.map(Ring::as_iovec).collect::<Vec<_>>();
    server_def.allocate_buffers(&vec);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_new() {
        let max_len = 100;
        let ring = Ring::new(max_len);
        assert_eq!(ring.data.len(), max_len);
        assert_eq!(ring.head, 0);
        assert_eq!(ring.max_len, max_len);
    }

    #[test]
    fn test_len_until_end() {
        let max_len = 100;
        let mut ring = Ring::new(max_len);
        assert_eq!(ring.len_until_end(), max_len);
        ring.head = 50;
        assert_eq!(ring.len_until_end(), 50);
    }

    #[test]
    fn test_get_contiguous() {
        let max_len = 100;
        let mut ring = Ring::new(max_len);

        // Test when len <= len_until_end
        let len = 50;
        let slice = ring.get_contiguous(len);
        assert_eq!(slice.len(), len);
        assert_eq!(ring.head, 0);

        // Test when len > len_until_end
        ring.head = 80;
        let len = 30;
        let slice = ring.get_contiguous(len);
        assert_eq!(slice.len(), len);
        assert_eq!(ring.head, 0);
    }

    #[test]
    fn test_advance() {
        let max_len = 100;
        let mut ring = Ring::new(max_len);

        // Test when head + len < max_len
        let len = 50;
        ring.advance(len);
        assert_eq!(ring.head, len);

        // Test when head + len >= max_len
        let len = 60;
        let prev_head = ring.head;
        ring.advance(len);
        assert_eq!(ring.head, (len + prev_head) % max_len);
    }

    #[test]
    fn test_append() {
        let max_len = 100;
        let mut ring = Ring::new(max_len);

        // Test appending data
        let data = b"Hello, World!";
        let ptr = ring.append(data);
        let appended_data = unsafe { std::slice::from_raw_parts(ptr, data.len()) };
        assert_eq!(appended_data, data);
        assert_eq!(ring.head, data.len());

        // Test appending data that wraps around
        let data2 = b"This is a longer string that will wrap around.";
        let ptr2 = ring.append(data2);
        let appended_data2 = unsafe { std::slice::from_raw_parts(ptr2, data2.len()) };
        assert_eq!(appended_data2, data2);
        assert_eq!(ring.head, (data.len() + data2.len()) % max_len);
    }
}
