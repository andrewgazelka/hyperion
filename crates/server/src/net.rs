//! All the networking related code.

#![expect(clippy::future_not_send, reason = "monoio is not Send")]

use std::{
    alloc::{alloc_zeroed, handle_alloc_error, Layout},
    borrow::Cow,
    cell::{Cell, UnsafeCell},
    ffi::c_void,
    io,
    io::{ErrorKind, Write},
    marker::PhantomData,
    mem::ManuallyDrop,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    os::fd::{AsRawFd, RawFd},
    ptr::addr_of_mut,
    sync::{
        atomic::{AtomicU16, AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{ensure, Context};
use arrayvec::ArrayVec;
use base64::Engine;
use bytes::BytesMut;
use derive_build::Build;
use evenio::prelude::Component;
use io_uring::{
    cqueue::buffer_select,
    squeue::SubmissionQueue,
    types::{BufRingEntry, Fixed},
    IoUring,
};
use libc::iovec;
use monoio::{
    buf::IoVecBuf,
    io::{
        AsyncReadRent, AsyncWriteRent, AsyncWriteRentExt, OwnedReadHalf, OwnedWriteHalf, Splitable,
    },
    FusionRuntime,
};
use serde_json::json;
use sha2::Digest;
use tracing::{debug, error, info, instrument, trace, warn};
use valence_protocol::{
    decode::PacketFrame,
    packets::{
        handshaking::{handshake_c2s::HandshakeNextState, HandshakeC2s},
        login,
        login::{LoginHelloC2s, LoginSuccessS2c},
        status,
    },
    uuid::Uuid,
    Bounded, CompressionThreshold, Decode, Encode, PacketDecoder, PacketEncoder, VarInt,
};

use crate::{config, global, global::Global};

/// Default MiB/s threshold before we start to limit the sending of some packets.
const DEFAULT_SPEED: u32 = 1024 * 1024;

/// The maximum number of buffers a vectored write can have.
const MAX_VECTORED_WRITE_BUFS: usize = 16;

const COMPLETION_QUEUE_SIZE: u32 = 32768;
const SUBMISSION_QUEUE_SIZE: u32 = 32768;
const IO_URING_FILE_COUNT: u32 = 32768;
const C2S_RING_BUFFER_COUNT: usize = 16384;

/// Size of each buffer in bytes
const C2S_RING_BUFFER_LEN: usize = 4096;

const LISTENER_FIXED_FD: Fixed = Fixed(0);
const C2S_BUFFER_GROUP_ID: u16 = 0;

const IORING_CQE_F_MORE: u32 = 1 << 1;

/// How long we wait from when we get the first buffer to when we start sending all of the ones we have collected.
/// This is closely related to [`MAX_VECTORED_WRITE_BUFS`].
const WRITE_DELAY: Duration = Duration::from_millis(1);

/// How much we expand our read buffer each time a packet is too large.
const READ_BUF_SIZE: usize = 4096;

/// The Minecraft protocol version this library currently targets.
pub const PROTOCOL_VERSION: i32 = 763;

/// The maximum number of bytes that can be sent in a single packet.
pub const MAX_PACKET_SIZE: usize = 0x001F_FFFF;

/// The stringified name of the Minecraft version this library currently
/// targets.
pub const MINECRAFT_VERSION: &str = "1.20.1";

/// Get a [`Uuid`] based on the given user's name.
fn offline_uuid(username: &str) -> anyhow::Result<Uuid> {
    let digest = sha2::Sha256::digest(username);

    #[expect(clippy::indexing_slicing, reason = "sha256 is always 32 bytes")]
    let slice = &digest[..16];

    Uuid::from_slice(slice).context("failed to create uuid")
}

/// Sent from the I/O thread when it has established a connection with the player through a handshake
pub struct ClientConnection {
    /// The local encoder used by that player
    pub encoder: Encoder,
    /// The name of the player.
    pub name: Box<str>,
    /// The UUID of the player.
    pub uuid: Uuid,
}

mod encoder;

#[derive(Component)]
pub struct Encoder {
    /// The encoding buffer and logic
    enc: encoder::PacketEncoder,

    /// If we should clear the `enc` allocation once we are done sending it off.
    ///
    /// In the future, perhaps we will have a global buffer if it is performant enough.
    deallocate_on_process: bool,
}

impl Encoder {
    /// Encode a packet.
    ///
    /// Returns if the buffer re-allocated
    fn encode_impl_allocates<P>(&mut self, pkt: &P) -> anyhow::Result<bool>
    where
        P: valence_protocol::Packet + Encode,
    {
        let capacity_before = self.enc.buf().capacity();
        self.enc.append_packet(pkt)?;
        let capacity_after = self.enc.buf().capacity();

        if capacity_before != capacity_after {
            return Ok(true);
        }

        Ok(false)
    }

    fn encode<P>(&mut self, pkt: &P, global: &Global) -> anyhow::Result<()> {
        if self.encode_impl_allocates(pkt)? {
            // todo: use thread-local instead of an atomic and then combine at the end
            global.set_needs_realloc()
        }
        Ok(())
    }

    unsafe fn as_iovec(&self) -> iovec {
        iovec {
            iov_base: self.enc.buf().as_ptr() as *mut c_void,
            iov_len: self.enc.buf().len(),
        }
    }

    /// This sends the bytes to the connection.
    /// [`PacketEncoder`] can have compression enabled.
    /// One must make sure the bytes are pre-compressed if compression is enabled.
    pub fn append(&mut self, bytes: &[u8]) {
        trace!("send raw bytes");
        self.enc.append_bytes(bytes);
    }
}

fn page_size() -> usize {
    // SAFETY: This is valid
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

fn alloc_zeroed_page_aligned<T>(len: usize) -> *mut T {
    assert!(len > 0);
    let page_size = page_size();
    let type_layout = Layout::new::<T>();
    assert!(type_layout.align() <= page_size);
    assert!(type_layout.size() > 0);

    let layout = Layout::from_size_align(len * type_layout.size(), page_size).unwrap();

    // SAFETY: len is nonzero and T is not zero sized
    let data = unsafe { alloc_zeroed(layout) };

    if data.is_null() {
        handle_alloc_error(layout);
    }

    data.cast()
}

pub struct MaybeRegisteredBuffer {
    registered_buffer: Vec<u8>,
    new_buffer: Option<Vec<u8>>,
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

impl MaybeRegisteredBuffer {
    fn with_capacity(len: usize) -> Self {
        Self {
            registered_buffer: Vec::new(), // no allocation
            new_buffer: Some(Vec::with_capacity(len)),
        }
    }

    fn register(&mut self) -> iovec {
        if let Some(buffer) = self.new_buffer.take() {
            self.registered_buffer = buffer;
        }

        iovec {
            iov_base: self.registered_buffer.as_ptr() as *mut c_void,
            iov_len: self.registered_buffer.capacity(),
        }
    }

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

        let Some(buffer) = self.registered_buffer else {
            unreachable!("this should never happen");
        };

        let cap = buffer.capacity();
        if cap < buffer.len() + bytes.len() {
            // copy buffer to new buffer
            let mut new_buffer = Vec::with_capacity(buffer.len() + bytes.len());
            new_buffer.extend_from_slice(&buffer);
            new_buffer.extend_from_slice(bytes);
            self.new_buffer = Some(new_buffer);
            return;
        }

        buffer.extend_from_slice(bytes);
    }
}

pub enum ServerEvent<'a> {
    AddPlayer { fd: Fixed },
    RemovePlayer { fd: Fixed },
    RecvData { fd: Fixed, data: &'a [u8] },
}

pub struct Server {
    listener: TcpListener,
    uring: IoUring,

    c2s_buffer: *mut [UnsafeCell<u8>; C2S_RING_BUFFER_LEN],
    c2s_local_tail: u16,
    c2s_shared_tail: *const AtomicU16,

    /// Make Listener !Send and !Sync to let io_uring assume that it'll only be accessed by 1
    /// thread
    phantom: PhantomData<*const ()>,
}

impl Server {
    pub fn new(address: impl ToSocketAddrs) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(address)?;
        // TODO: Try to use defer taskrun
        let mut uring = IoUring::builder()
            .setup_cqsize(COMPLETION_QUEUE_SIZE)
            .setup_submit_all()
            .setup_coop_taskrun()
            .setup_single_issuer()
            .build(SUBMISSION_QUEUE_SIZE)
            .unwrap();

        let mut submitter = uring.submitter();
        submitter.register_files_sparse(IO_URING_FILE_COUNT)?;
        assert_eq!(
            submitter.register_files_update(LISTENER_FIXED_FD.0, &[listener.as_raw_fd()])?,
            1
        );

        // Create the c2s buffer
        let c2s_buffer = alloc_zeroed_page_aligned::<[UnsafeCell<u8>; C2S_RING_BUFFER_LEN]>(
            C2S_RING_BUFFER_COUNT,
        );
        let buffer_ring = alloc_zeroed_page_aligned::<BufRingEntry>(C2S_RING_BUFFER_COUNT);
        {
            let c2s_buffer =
                unsafe { std::slice::from_raw_parts(c2s_buffer, C2S_RING_BUFFER_COUNT) };

            // SAFETY: Buffer count is smaller than the entry count, BufRingEntry is initialized with
            // zero, and the underlying will not be mutated during the loop
            let buffer_ring =
                unsafe { std::slice::from_raw_parts_mut(buffer_ring, C2S_RING_BUFFER_COUNT) };

            for (buffer_id, buffer) in buffer_ring.into_iter().enumerate() {
                let underlying_data = &c2s_buffer[buffer_id];
                buffer.set_addr(underlying_data.as_ptr() as u64);
                buffer.set_len(underlying_data.len() as u32);
                buffer.set_bid(buffer_id as u16);
            }
        }

        let tail = C2S_RING_BUFFER_COUNT as u16;

        // Update the tail
        // SAFETY: This is the first entry of the buffer ring
        let tail_addr = unsafe { BufRingEntry::tail(buffer_ring) };

        // SAFETY: tail_addr doesn't need to be atomic since it hasn't been passed to the kernel
        // yet
        unsafe {
            *tail_addr.cast_mut() = tail;
        }

        let tail_addr: *const AtomicU16 = tail_addr.cast();

        // Register the buffer ring
        // SAFETY: buffer_ring is valid to write to for C2S_RING_BUFFER_COUNT BufRingEntry structs
        unsafe {
            submitter.register_buf_ring(
                buffer_ring as u64,
                C2S_RING_BUFFER_COUNT as u16,
                C2S_BUFFER_GROUP_ID,
            )?;
        }

        Self::request_accept(&mut uring.submission());

        Ok(Self {
            listener,
            uring,
            c2s_buffer,
            c2s_local_tail: tail,
            c2s_shared_tail: tail_addr,
            phantom: PhantomData,
        })
    }

    /// # Safety
    /// The entry must be valid for the duration of the operation
    unsafe fn push_entry(submission: &mut SubmissionQueue, entry: &io_uring::squeue::Entry) {
        loop {
            if submission.push(entry).is_ok() {
                return;
            }

            // The submission queue is full. Let's try syncing it to see if the size is reduced
            submission.sync();

            if submission.push(entry).is_ok() {
                return;
            }

            // The submission queue really is full. The submission queue should be large enough so that
            // this code is never reached.
            warn!(
                "io_uring submission queue is full and this will lead to performance issues; \
                 consider increasing SUBMISSION_QUEUE_SIZE to avoid this"
            );
            std::hint::spin_loop();
        }
    }

    fn request_accept(submission: &mut SubmissionQueue) {
        unsafe {
            Self::push_entry(
                submission,
                &io_uring::opcode::AcceptMulti::new(LISTENER_FIXED_FD)
                    .allocate_file_index(true)
                    .build()
                    .user_data(0),
            );
        }
    }

    fn request_recv(submission: &mut SubmissionQueue, fd: Fixed) {
        unsafe {
            Self::push_entry(
                submission,
                &io_uring::opcode::RecvMulti::new(fd, C2S_BUFFER_GROUP_ID)
                    .build()
                    .user_data((fd.0 + 2) as u64),
            );
        }
    }

    pub fn write_raw(&mut self, fd: Fixed, buf: *const u8, len: u32, buf_index: u16) {
        unsafe {
            Self::push_entry(
                &mut self.uring.submission(),
                &io_uring::opcode::WriteFixed::new(fd, buf, len, buf_index)
                    .build()
                    .user_data(1),
            );
        }
    }

    pub fn submit_events(&mut self) {
        if let Err(err) = self.uring.submit() {
            error!("unexpected io_uring error during submit: {err}");
        }
    }

    pub fn drain<F: FnMut(ServerEvent)>(&mut self, mut f: F) {
        let (_, mut submission, mut completion) = self.uring.split();
        completion.sync();
        if completion.overflow() > 0 {
            error!(
                "the io_uring completion queue overflowed, and some connection errors are likely \
                 to occur; consider increasing COMPLETION_QUEUE_SIZE to avoid this"
            );
        }

        for event in completion {
            match event.user_data() {
                0 => {
                    // `IORING_CQE_F_MORE` is a flag used in the context of the io_uring asynchronous I/O framework,
                    // which is a Linux kernel feature.
                    // This flag is specifically related to completion queue events (CQEs).
                    // When `IORING_CQE_F_MORE` is set in a CQE,
                    // it indicates that there are more completion events to be processed after the current one.
                    // This is particularly useful in scenarios
                    // where multiple I/O operations are being completed at once,
                    // allowing for more efficient processing
                    // by enabling the application
                    // to handle several completion events in a batch-like manner
                    // before needing to recheck the completion queue.
                    //
                    // The use of `IORING_CQE_F_MORE`
                    // can enhance performance in high-throughput I/O environments
                    // by reducing the overhead of accessing the completion queue multiple times.
                    // Instead, you can gather and process multiple completions in a single sweep.
                    // This is especially advantageous in systems where minimizing latency
                    // and maximizing throughput are critical,
                    // such as in database management systems or high-performance computing applications.
                    if event.flags() & IORING_CQE_F_MORE == 0 {
                        warn!("multishot accept rerequested");
                        Self::request_accept(&mut submission);
                    }

                    if event.result() < 0 {
                        error!("there was an error in accept: {}", event.result());
                    } else {
                        let fd = Fixed(event.result() as u32);
                        Self::request_recv(&mut submission, fd);
                        f(ServerEvent::AddPlayer { fd });
                    }
                }
                1 => {
                    // TODO: check for errors and, if not all bytes were written or the request was
                    // cancelled, close the client socket
                    warn!("got write response");
                }
                fd_plus_2 => {
                    let fd = Fixed((fd_plus_2 - 2) as u32);
                    let disconnected = event.result() == 0;

                    if event.flags() & IORING_CQE_F_MORE == 0 && !disconnected {
                        warn!("socket recv rerequested");
                        Self::request_recv(&mut submission, fd);
                    }

                    if disconnected {
                        f(ServerEvent::RemovePlayer { fd });
                    } else if event.result() < 0 {
                        error!("there was an error in recv: {}", event.result());
                    } else {
                        let bytes_received = event.result() as usize;
                        let buffer_id =
                            buffer_select(event.flags()).expect("there should be a buffer");
                        assert!((buffer_id as usize) < C2S_RING_BUFFER_COUNT);
                        // TODO: this is probably very unsafe
                        let buffer = unsafe {
                            *(self.c2s_buffer.add(buffer_id as usize)
                                as *const [u8; C2S_RING_BUFFER_LEN])
                        };
                        let buffer = &buffer[..bytes_received];
                        self.c2s_local_tail = self.c2s_local_tail.wrapping_add(1);
                        f(ServerEvent::RecvData { fd, data: buffer });
                    }
                }
            }
        }

        // SAFETY: c2s_shared_tail is valid
        unsafe {
            (*self.c2s_shared_tail).store(self.c2s_local_tail, Ordering::Relaxed);
        }
    }

    pub fn cancel(&mut self, cancel_builder: io_uring::types::CancelBuilder) {
        self.uring
            .submitter()
            .register_sync_cancel(None, cancel_builder)
            .unwrap();
    }

    pub fn refresh_buffers(
        &mut self,
        global: &Global,
        encoders: impl Iterator<Item = &mut Encoder>,
    ) {
        if !global.get_needs_realloc() {
            return;
        }

        self.unregister_buffers();

        let encoders: Vec<_> = encoders.map(|encoder| encoder.enc.buf.register()).collect();

        unsafe { self.register_buffers(&encoders) };
    }

    /// To register new buffers, unregister must be called first
    /// # Safety
    /// buffers must be valid
    pub unsafe fn register_buffers(&mut self, buffers: &[iovec]) {
        self.uring.submitter().register_buffers(buffers).unwrap();
    }

    /// All requests in the submission queue must be finished or cancelled, or else this function
    /// will hang indefinetely.
    pub fn unregister_buffers(&mut self) {
        self.uring.submitter().unregister_buffers().unwrap();
    }
}
