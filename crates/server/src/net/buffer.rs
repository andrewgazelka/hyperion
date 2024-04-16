use std::{
    ffi::c_void,
    io,
    io::Write,
    ops::{Index, IndexMut, RangeBounds},
    slice::SliceIndex,
};

use bytes::BufMut;
use libc::iovec;

pub struct MaybeRegisteredBuffer {
    registered_buffer: Vec<u8>,
    new_buffer: Option<Vec<u8>>,
}

impl<T: SliceIndex<[u8]>> Index<T> for MaybeRegisteredBuffer {
    type Output = T::Output;

    fn index(&self, index: T) -> &Self::Output {
        &self.current_buffer()[index]
    }
}

impl<T: SliceIndex<[u8]>> IndexMut<T> for MaybeRegisteredBuffer {
    fn index_mut(&mut self, index: T) -> &mut Self::Output {
        &mut self.current_buffer_mut()[index]
    }
}

impl MaybeRegisteredBuffer {
    const fn current_buffer(&self) -> &Vec<u8> {
        if let Some(buffer) = &self.new_buffer {
            buffer
        } else {
            &self.registered_buffer
        }
    }
    
    pub const fn needs_realloc(&self) -> bool {
        self.new_buffer.is_some()
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.current_buffer().as_ptr()
    }

    pub fn len(&self) -> usize {
        self.current_buffer().len()
    }

    pub fn copy_within(&mut self, range: impl RangeBounds<usize>, offset: usize) {
        let buffer = self.current_buffer_mut();
        buffer.copy_within(range, offset);
    }

    fn add_capacity(&mut self, add: usize) -> &mut Vec<u8> {
        if self.new_buffer.is_some() {
            return self.new_buffer.as_mut().unwrap();
        }

        if self.registered_buffer.capacity() < self.registered_buffer.len() + add {
            return &mut self.registered_buffer;
        }

        let mut new_buffer = Vec::with_capacity(self.registered_buffer.len() + add);
        new_buffer.extend_from_slice(&self.registered_buffer);
        self.new_buffer = Some(new_buffer);

        self.new_buffer.as_mut().unwrap()
    }

    pub fn put_bytes(&mut self, byte: u8, amount: usize) {
        self.add_capacity(amount).put_bytes(byte, amount);
    }

    fn current_buffer_mut(&mut self) -> &mut Vec<u8> {
        if let Some(buffer) = &mut self.new_buffer {
            buffer
        } else {
            &mut self.registered_buffer
        }
    }

    pub fn extend_from_slice(&mut self, slice: &[u8]) {
        self.current_buffer_mut().extend_from_slice(slice);
    }

    pub fn truncate(&mut self, len: usize) {
        self.current_buffer_mut().truncate(len);
    }

    fn with_capacity(len: usize) -> Self {
        Self {
            registered_buffer: Vec::new(), // no allocation
            new_buffer: Some(Vec::with_capacity(len)),
        }
    }

    #[allow(clippy::as_ptr_cast_mut, reason = "pretty sure nursery error")]
    pub fn as_iovec(&mut self) -> Option<iovec> {
        if let Some(buffer) = self.new_buffer.take() {
            self.registered_buffer = buffer;
        }

        let capacity = self.registered_buffer.capacity();

        // if capacity is 0, there is no allocation
        if capacity == 0 {
            return None;
        }

        Some(iovec {
            iov_base: self.registered_buffer.as_ptr() as *mut c_void,
            iov_len: self.registered_buffer.capacity(),
        })
    }

    #[allow(clippy::as_ptr_cast_mut, reason = "pretty sure nursery error")]
    fn get_iovec(&self) -> iovec {
        iovec {
            iov_base: self.registered_buffer.as_ptr() as *mut c_void,
            iov_len: self.registered_buffer.len(),
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        // todo: could be made more efficient with custom Vec that does not instantly deallocate on grow
        if let Some(buffer) = &mut self.new_buffer {
            buffer.extend_from_slice(bytes);
            return;
        }

        let buffer = &mut self.registered_buffer;

        let cap = buffer.capacity();
        if cap < buffer.len() + bytes.len() {
            // copy buffer to new buffer
            let mut new_buffer = Vec::with_capacity(buffer.len() + bytes.len());
            new_buffer.extend_from_slice(buffer);
            new_buffer.extend_from_slice(bytes);
            self.new_buffer = Some(new_buffer);
            return;
        }

        buffer.extend_from_slice(bytes);
    }
}

impl Default for MaybeRegisteredBuffer {
    fn default() -> Self {
        Self {
            registered_buffer: Vec::new(),
            new_buffer: Some(Vec::new()),
        }
    }
}

impl Write for MaybeRegisteredBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.push(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
