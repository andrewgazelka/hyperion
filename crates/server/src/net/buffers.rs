use std::{
    cell::UnsafeCell,
    ffi::c_void,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use arraydeque::ArrayDeque;
use derive_more::{Deref, DerefMut};
use evenio::prelude::Component;
use libc::iovec;
use rayon_local::RayonLocal;
use thiserror::Error;
use tracing::{instrument, span, trace, Level};

use crate::{
    net::{Server, ServerDef, Servers, MAX_PACKET_SIZE},
    singleton::ring::Ring,
};

const COUNT: usize = 20;

const BUFFER_SIZE: usize = MAX_PACKET_SIZE * 2;

pub struct BufferAllocator {
    // todo: see if there is a way to avoid Rc and just use &'a BufferAllocatorInner
    inner: Arc<BufferAllocatorInner>,
}

#[derive(Component, Deref, DerefMut)]
pub struct BufferAllocators {
    inner: RayonLocal<BufferAllocator>,
}

impl BufferAllocators {
    #[must_use]
    pub fn new(server_def: &Servers) -> Self {
        let new_span = span!(Level::TRACE, "BAS");
        let _enter = new_span.enter();

        let inner = RayonLocal::none();

        rayon::broadcast(|_| {
            let server = server_def.get_local_raw();
            let server = unsafe { &mut *server.get() };

            let index = rayon::current_thread_index().unwrap();

            let result = span!(parent: &new_span, Level::TRACE, "new", index = index)
                .in_scope(|| BufferAllocator::new(server));

            let inner = inner.get_local_raw();
            let inner = unsafe { &mut *inner.get() };

            *inner = Some(result);
        });

        let inner = inner.unwrap_all();

        Self { inner }
    }
}

#[derive(Error, Debug)]
pub enum BufferAllocationError {
    #[error("all buffers are being used")]
    AllBuffersInUse,
}

impl BufferAllocator {
    #[instrument(skip_all)]
    pub fn obtain(&mut self) -> Result<BufRef, BufferAllocationError> {
        #[cfg(debug_assertions)]
        {
            let buffers_left = self.inner.available.lock().len();
            // trace!("buffers left: {buffers_left}");
        }

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

struct BufferAllocatorInner {
    // todo: try on stack? will probs need to increase stack size. idk if this even makes sense to do though.
    // todo: probs just have one continuous buffer and then something that is similar to an arrayvec but references it
    buffers: Box<[UnsafeCell<Ring<BUFFER_SIZE>>]>,
    available: parking_lot::Mutex<ArrayDeque<u16, COUNT>>,
}

// I think this is safe because indices of buffer are never accessed concurrently
unsafe impl Send for BufferAllocatorInner {}
unsafe impl Sync for BufferAllocatorInner {}

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
