use std::{
    cell::UnsafeCell,
    ffi::c_void,
    fmt::Debug,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use arrayvec::ArrayVec;
use evenio::prelude::Component;
use libc::iovec;

use crate::net::ServerDef;

const COUNT: usize = 1024;

const BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Component)]
pub struct BufferAllocator {
    // todo: see if there is a way to avoid Rc and just use &'a BufferAllocatorInner
    inner: Rc<BufferAllocatorInner>,
}

// TODO: REMOVE
unsafe impl Send for BufferAllocator {}
unsafe impl Sync for BufferAllocator {}

impl BufferAllocator {
    pub fn obtain(&self) -> Option<BufRef> {
        let index = unsafe { &mut *self.inner.available.get() }.pop()?;

        Some(BufRef {
            index,
            allocator: self.inner.clone(),
        })
    }

    pub fn new(server_def: &mut impl ServerDef) -> Self {
        let inner = BufferAllocatorInner::new(server_def);
        Self {
            inner: Rc::new(inner),
        }
    }
}

struct BufferAllocatorInner {
    // todo: try on stack? will probs need to increase stack size. idk if this even makes sense to do though.
    buffers: Box<[UnsafeCell<ArrayVec<u8, BUFFER_SIZE>>]>,
    available: UnsafeCell<ArrayVec<u16, COUNT>>,
}

pub struct BufRef {
    index: u16,
    allocator: Rc<BufferAllocatorInner>,
}

impl Debug for BufRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufRef")
            .field("index", &self.index)
            .finish()
    }
}

impl BufRef {
    pub const fn index(&self) -> u16 {
        self.index
    }
}

impl Drop for BufRef {
    fn drop(&mut self) {
        self.clear();
        unsafe { &mut *self.allocator.available.get() }.push(self.index);
    }
}

impl Deref for BufRef {
    type Target = ArrayVec<u8, BUFFER_SIZE>;

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
        let available = std::array::from_fn(|i| i as u16);

        let buffers: Box<_> = (0..COUNT)
            .map(|_| UnsafeCell::new(ArrayVec::new()))
            .collect();

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
            available: UnsafeCell::new(ArrayVec::from(available)),
        }
    }
}
