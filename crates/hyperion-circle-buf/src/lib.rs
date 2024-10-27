use std::{
    alloc::{self, Layout},
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

/// A lock-free SPSC (Single Producer Single Consumer) ring buffer optimized for HPC scenarios.
/// Specialized for u8 data to optimize for common byte stream use cases.
pub struct RingBuf {
    buf: NonNull<u8>,
    capacity: usize,
    mask: usize,
    // Position that has been committed/made visible to reader
    committed: AtomicUsize,
}

/// Writer handle for pushing data into the buffer
pub struct Producer {
    inner: *const RingBuf,
    // Local write position for batching writes
    write_pos: usize,
}

unsafe impl Send for Producer {}

/// Reader handle for consuming data from the buffer
pub struct Consumer {
    inner: *const RingBuf,
    // Local read position
    read_pos: usize,
}

unsafe impl Send for Consumer {}

// Safety: RingBuf can be sent between threads
unsafe impl Send for RingBuf {}
unsafe impl Sync for RingBuf {}

#[must_use]
pub fn new_pair(capacity: usize) -> (Producer, Consumer) {
    let ring = RingBuf::new(capacity);
    RingBuf::split(Box::new(ring))
}

impl RingBuf {
    /// Creates a new [`RingBuf`] with the specified capacity rounded up to the next power of 2
    #[must_use]
    pub fn new(mut capacity: usize) -> Self {
        // Round up to next power of 2 for efficient wrapping
        capacity = capacity.next_power_of_two();
        let layout = Layout::array::<u8>(capacity).unwrap();
        let buf = unsafe { NonNull::new(alloc::alloc(layout)).expect("allocation failed") };

        Self {
            buf,
            capacity,
            mask: capacity - 1,
            committed: AtomicUsize::new(0),
        }
    }

    /// Creates a producer-consumer pair for this buffer
    #[must_use]
    pub fn split(buffer: Box<Self>) -> (Producer, Consumer) {
        let ptr = Box::into_raw(buffer);
        (
            Producer {
                inner: ptr,
                write_pos: 0,
            },
            Consumer {
                inner: ptr,
                read_pos: 0,
            },
        )
    }
}

impl Producer {
    /// Returns two slices representing the available write space:
    /// - First slice: from write position to end of buffer
    /// - Second slice: from start of buffer to wrap point
    #[inline]
    #[must_use]
    pub fn write_bufs(&self) -> [&mut [u8]; 2] {
        let ring = unsafe { &*self.inner };
        let write_idx = self.write_pos & ring.mask;

        unsafe {
            let ptr = ring.buf.as_ptr();

            // First slice: from write_idx to end of buffer
            let slice1 =
                std::slice::from_raw_parts_mut(ptr.add(write_idx), ring.capacity - write_idx);

            // Second slice: from start to write_idx
            let slice2 = std::slice::from_raw_parts_mut(ptr, write_idx);

            [slice1, slice2]
        }
    }

    /// Commits written data to be visible to the consumer
    #[inline]
    pub fn commit(&mut self, written: usize) {
        let ring = unsafe { &*self.inner };
        self.write_pos = self.write_pos.wrapping_add(written);
        ring.committed.store(self.write_pos, Ordering::Release);
    }

    /// Write as much data as possible from the slice, returns number of bytes written
    #[inline]
    pub fn write(&mut self, src: &[u8]) -> usize {
        if src.is_empty() {
            return 0;
        }

        let [buf1, buf2] = self.write_bufs();
        let mut written = 0;

        // Write to first buffer
        let to_write1 = src.len().min(buf1.len());
        if to_write1 > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(src.as_ptr(), buf1.as_mut_ptr(), to_write1);
            }
            written += to_write1;
        }

        // Write remaining data to second buffer if needed
        let remaining = src.len() - written;
        if remaining > 0 {
            let to_write2 = remaining.min(buf2.len());
            if to_write2 > 0 {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        src.as_ptr().add(written),
                        buf2.as_mut_ptr(),
                        to_write2,
                    );
                }
                written += to_write2;
            }
        }

        // Commit the write
        self.commit(written);
        written
    }

    #[inline]
    #[must_use]
    pub fn capacity(&self) -> usize {
        unsafe { (*self.inner).capacity }
    }
}

impl Consumer {
    /// Returns a slice containing available data to read
    #[inline]
    #[must_use]
    pub fn read_buf(&self) -> &[u8] {
        let ring = unsafe { &*self.inner };
        let read_idx = self.read_pos & ring.mask;

        // Get committed write position
        let committed = ring.committed.load(Ordering::Acquire);
        let available = committed.wrapping_sub(self.read_pos);

        if available == 0 {
            return &[];
        }

        // Calculate contiguous readable bytes
        let readable = available.min(ring.capacity - read_idx);

        unsafe { std::slice::from_raw_parts(ring.buf.as_ptr().add(read_idx), readable) }
    }

    /// Advances the read position
    #[inline]
    pub fn commit(&mut self, amount: usize) {
        self.read_pos = self.read_pos.wrapping_add(amount);
    }

    /// Returns number of bytes available to read
    #[inline]
    #[must_use]
    pub fn available(&self) -> usize {
        let ring = unsafe { &*self.inner };
        let committed = ring.committed.load(Ordering::Acquire);
        committed.wrapping_sub(self.read_pos)
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.available() == 0
    }
}

impl Drop for RingBuf {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::array::<u8>(self.capacity).unwrap();
            alloc::dealloc(self.buf.as_ptr(), layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn test_basic_operations() {
        let buffer = Box::new(RingBuf::new(16));
        let (mut producer, mut consumer) = RingBuf::split(buffer);

        // Test writing
        assert_eq!(producer.write(&[1, 2, 3, 4]), 4);

        // Test reading
        let data = consumer.read_buf();
        assert_eq!(data, &[1, 2, 3, 4]);

        // Test consuming
        consumer.commit(2);
        let data = consumer.read_buf();
        assert_eq!(data, &[3, 4]);
    }

    #[test]
    fn test_wrap_around() {
        let buffer = Box::new(RingBuf::new(4));
        let (mut producer, mut consumer) = RingBuf::split(buffer);

        // Fill buffer
        assert_eq!(producer.write(&[1, 2, 3, 4]), 4);

        // Read part
        assert_eq!(consumer.read_buf(), &[1, 2, 3, 4]);
        consumer.commit(2);

        // Write more to trigger wrap
        assert_eq!(producer.write(&[5, 6]), 2);

        // Verify data
        assert_eq!(consumer.read_buf(), &[3, 4]);
        consumer.commit(2);
        assert_eq!(consumer.read_buf(), &[5, 6]);
    }

    #[test]
    fn test_concurrent_access() {
        let buffer = Box::new(RingBuf::new(1024));
        let (mut producer, mut consumer) = RingBuf::split(buffer);

        let producer_thread = thread::spawn(move || {
            let data: Vec<u8> = (0..100).collect();
            let mut written = 0;
            while written < data.len() {
                written += producer.write(&data[written..]);
            }
        });

        let consumer_thread = thread::spawn(move || {
            let mut total_read = 0;
            while total_read < 100 {
                let data = consumer.read_buf();
                if !data.is_empty() {
                    // Verify data
                    for (i, &byte) in data.iter().enumerate() {
                        assert_eq!(byte, u8::try_from(total_read + i).unwrap());
                    }
                    let len = data.len();
                    consumer.commit(data.len());
                    total_read += len;
                }
            }
        });

        producer_thread.join().unwrap();
        consumer_thread.join().unwrap();
    }
}
