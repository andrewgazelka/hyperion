use std::{
    cell::UnsafeCell,
    ffi::c_void,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use arraydeque::ArrayDeque;
use evenio::prelude::Component;
use libc::iovec;

use crate::{net::ServerDef, singleton::ring::Ring};

const COUNT: usize = 1024;

const BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Component)]
pub struct BufferAllocator {
    // todo: see if there is a way to avoid Rc and just use &'a BufferAllocatorInner
    inner: Arc<BufferAllocatorInner>,
}

impl BufferAllocator {
    pub fn obtain(&self) -> Option<BufRef> {
        let index = unsafe { &mut *self.inner.available.get() }.pop_front()?;

        Some(BufRef {
            index,
            allocator: self.inner.clone(),
        })
    }

    pub fn new(server_def: &mut impl ServerDef) -> Self {
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
    available: UnsafeCell<ArrayDeque<u16, COUNT>>,
}

// todo: is this correct?
unsafe impl Send for BufferAllocatorInner {}
unsafe impl Sync for BufferAllocatorInner {}

pub struct BufRef {
    index: u16,
    allocator: Arc<BufferAllocatorInner>,
}

impl BufRef {
    pub const fn index(&self) -> u16 {
        self.index
    }
}

impl Drop for BufRef {
    fn drop(&mut self) {
        // we are pushing back so we give the buffer the maximum lifetime
        // in io-uring
        unsafe { &mut *self.allocator.available.get() }
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
    fn new(server_def: &mut impl ServerDef) -> Self {
        let available: [u16; COUNT] = std::array::from_fn(|i| i as u16);

        let buffers: Box<_> = (0..COUNT).map(|_| UnsafeCell::new(Ring::new())).collect();

        let iovecs = buffers
            .iter()
            .map(|buffer| iovec {
                iov_base: buffer.get().cast::<c_void>(),
                iov_len: BUFFER_SIZE,
            })
            .collect::<Vec<_>>();

        server_def.allocate_buffers(&iovecs);

        Self {
            buffers,
            available: UnsafeCell::new(ArrayDeque::from(available)),
        }
    }
}
