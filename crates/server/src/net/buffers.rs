use std::{
    cell::UnsafeCell,
    ffi::c_void,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use arraydeque::ArrayDeque;
use evenio::prelude::Component;
use libc::iovec;
use thiserror::Error;
use tracing::instrument;

use crate::{
    net::{Server, ServerDef, MAX_PACKET_SIZE},
    singleton::ring::Ring,
};

const COUNT: usize = 1024;

const BUFFER_SIZE: usize = MAX_PACKET_SIZE * 4;

#[derive(Component)]
pub struct BufferAllocator {
    // todo: see if there is a way to avoid Rc and just use &'a BufferAllocatorInner
    inner: Arc<BufferAllocatorInner>,
}

#[derive(Error, Debug)]
pub enum BufferAllocationError {
    #[error("all buffers are being used")]
    AllBuffersInUse,
}

impl BufferAllocator {
    #[instrument(skip_all)]
    pub fn obtain(&mut self) -> Result<BufRef, BufferAllocationError> {
        let index = self
            .inner
            .available
            .lock()
            .pop_front()
            .ok_or(BufferAllocationError::AllBuffersInUse)?;

        Ok(BufRef {
            index,
            allocator: self.inner.clone(),
        })
    }

    pub fn new(server_def: &mut Server) -> Self {
        let inner = BufferAllocatorInner::new(server_def);
        Self {
            inner: Arc::new(inner),
        }
    }
}

#[derive(Debug)]
struct BufferAllocatorInner {
    // todo: try on stack? will probs need to increase stack size. idk if this even makes sense to do though.
    // todo: probs just have one continuous buffer and then something that is similar to an arrayvec but references it
    buffers: Box<[UnsafeCell<Ring<BUFFER_SIZE>>]>,
    available: parking_lot::Mutex<ArrayDeque<u16, COUNT>>,
}

// I think this is safe because indices of buffer are never accessed concurrently
unsafe impl Send for BufferAllocatorInner {}
unsafe impl Sync for BufferAllocatorInner {}

#[derive(Debug)]
pub struct BufRef {
    index: u16,
    allocator: Arc<BufferAllocatorInner>,
}

impl BufRef {
    #[must_use]
    pub const fn index(&self) -> u16 {
        self.index
    }
}

impl Drop for BufRef {
    fn drop(&mut self) {
        // we are pushing back so we give the buffer the maximum lifetime
        // in io-uring
        self.allocator
            .available
            .lock()
            .push_back(self.index)
            .unwrap();
    }
}

impl Deref for BufRef {
    type Target = Ring<BUFFER_SIZE>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.allocator.buffers[self.index as usize].get() }
    }
}

impl DerefMut for BufRef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.allocator.buffers[self.index as usize].get() }
    }
}

impl BufferAllocatorInner {
    #[allow(
        clippy::large_stack_frames,
        reason = "todo probs remove somehow but idk how"
    )]
    fn new(server_def: &mut Server) -> Self {
        // trace!("initializing buffer allocator");
        let available: [u16; COUNT] = std::array::from_fn(|i| i as u16);

        let buffers = Box::new_zeroed_slice(COUNT);
        let buffers: Box<[UnsafeCell<Ring<BUFFER_SIZE>>]> = unsafe { buffers.assume_init() };

        // trace!("got buffers");

        let iovecs = buffers
            .iter()
            .map(|buffer| iovec {
                iov_base: buffer.get().cast::<c_void>(),
                iov_len: BUFFER_SIZE,
            })
            .collect::<Vec<_>>();

        unsafe { server_def.register_buffers(&iovecs) };

        Self {
            buffers,
            available: parking_lot::Mutex::new(ArrayDeque::from(available)),
        }
    }
}
