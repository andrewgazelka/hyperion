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
    net::{Server, ServerDef},
};

// TODO: adjust
const BUFFER_SIZE: usize = 256 * 1024;

#[derive(Component)]
pub struct RegisteredBuffer {
    inner: Box<[u8]>
}

impl RegisteredBuffer {
    fn new(server: &mut Server) {
        let buffer = vec![0u8; BUFFER_SIZE].into_boxed_slice();
        unsafe {
            server.register_buffers(iovec {
                iov_base: buffer.as_mut_ptr().cast(),
                iov_len: buffer.len()
            });
        }
        Self {
            inner: buffer
        }
    }
}

impl DerefMut for RegisteredBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Deref for RegisteredBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
