//! All the networking related code.

use std::{
    alloc::{alloc_zeroed, handle_alloc_error, Layout},
    cell::UnsafeCell,
    cmp,
    marker::PhantomData,
    net::{TcpListener, ToSocketAddrs},
    os::fd::AsRawFd,
    sync::atomic::{AtomicU16, Ordering},
    time::Duration,
};

pub use io_uring::types::Fixed;
use io_uring::{cqueue::buffer_select, squeue::SubmissionQueue, types::BufRingEntry, IoUring};
use io_uring::squeue::Flags;
use libc::iovec;
use tracing::{error, info, warn};

use super::RefreshItem;
use crate::{
    global::Global,
    net::{Fd, ServerDef, ServerEvent},
    singleton::buffer_allocator::BufRef,
};

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

fn page_size() -> usize {
    Flags::IO_LINK
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

pub struct LinuxServer {
    listener: TcpListener,
    uring: IoUring,

    c2s_buffer: *mut [UnsafeCell<u8>; C2S_RING_BUFFER_LEN],
    c2s_local_tail: u16,
    c2s_shared_tail: *const AtomicU16,

    /// Make Listener !Send and !Sync to let io_uring assume that it'll only be accessed by 1
    /// thread
    phantom: PhantomData<*const ()>,
}

// TODO: REMOVE
unsafe impl Send for LinuxServer {}
unsafe impl Sync for LinuxServer {}

impl ServerDef for LinuxServer {
    fn new(address: impl ToSocketAddrs) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(address)?;

        let addr = listener.local_addr()?;
        println!("starting on {addr:?}");

        // TODO: Try to use defer taskrun
        let mut uring = IoUring::builder()
            .setup_cqsize(COMPLETION_QUEUE_SIZE)
            .setup_submit_all()
            .setup_coop_taskrun()
            .setup_single_issuer()
            .build(SUBMISSION_QUEUE_SIZE)
            .unwrap();

        let submitter = uring.submitter();
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

    fn drain(&mut self, mut f: impl FnMut(ServerEvent)) {
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
                    if event.flags() & IORING_CQE_F_MORE == 0 {
                        warn!("multishot accept rerequested");
                        Self::request_accept(&mut submission);
                    }

                    if event.result() < 0 {
                        error!("there was an error in accept: {}", event.result());
                    } else {
                        let fd = Fixed(event.result() as u32);
                        Self::request_recv(&mut submission, fd);
                        f(ServerEvent::AddPlayer { fd: Fd(fd) });
                    }
                }
                write if write & SEND_MARKER != 0 => {
                    let fd = Fixed((write & !SEND_MARKER) as u32);
                    let result = event.result();

                    match result.cmp(&0) {
                        cmp::Ordering::Less => {
                            error!("there was an error in write: {}", result);
                        }
                        cmp::Ordering::Equal => {
                            // A result of 0 indicates that the client closed the connection gracefully
                            f(ServerEvent::RemovePlayer { fd: Fd(fd) });
                            // Perform any necessary cleanup for the disconnected client
                        }
                        cmp::Ordering::Greater => {
                            // Write operation completed successfully
                            info!("successful write response");
                        }
                    }
                }
                read if read & RECV_MARKER != 0 => {
                    let fd = Fixed((read & !RECV_MARKER) as u32);
                    let disconnected = event.result() == 0;

                    if event.flags() & IORING_CQE_F_MORE == 0 && !disconnected {
                        info!("socket recv rerequested");
                        Self::request_recv(&mut submission, fd);
                    }

                    if disconnected {
                        f(ServerEvent::RemovePlayer { fd: Fd(fd) });
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
                        f(ServerEvent::RecvData {
                            fd: Fd(fd),
                            data: buffer,
                        });
                    }
                }
                _ => {
                    panic!("unexpected event: {event:?}");
                }
            }
        }

        // SAFETY: c2s_shared_tail is valid
        unsafe {
            (*self.c2s_shared_tail).store(self.c2s_local_tail, Ordering::Relaxed);
        }
    }

    fn allocate_buffers(&mut self, buffers: &[iovec]) {
        println!("allocate buffers");
        unsafe { self.register_buffers(buffers) };
    }

    fn write<'a>(&mut self, _global: &mut Global, items: impl Iterator<Item = RefreshItem<'a>>) {
        self.write_all(items);
    }

    fn broadcast(&mut self, buf: &BufRef, fds: impl Iterator<Item = Fd>) {
        if buf.len() == 0 {
            return;
        }

        let location = buf.as_ptr();
        let idx = buf.index();
        let len = buf.len() as u32;

        if len == 0 {
            return;
        }

        fds.for_each(|fd| {
            let fd = fd.0;
            self.write_raw(fd, location, len, idx);
        });
    }

    fn submit_events(&mut self) {
        if let Err(err) = self.uring.submit() {
            error!("unexpected io_uring error during submit: {err}");
        }
    }
}

const RECV_MARKER: u64 = 0b1 << 63;
const SEND_MARKER: u64 = 0b1 << 62;

impl LinuxServer {
    fn write_all<'a>(&mut self, items: impl Iterator<Item = RefreshItem<'a>>) {
        items.for_each(|(buf, fd)| {
            let fd = fd.0;

            if buf.len() == 0 {
                return;
            }

            let location = buf.as_ptr();
            let idx = buf.index();
            let len = buf.len() as u32;

            if len == 0 {
                return;
            }

            self.write_raw(fd, location, len, idx);
        });
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
                    .user_data((fd.0 as u64) | RECV_MARKER),
            );
        }
    }

    pub fn write_raw(&mut self, fd: Fixed, buf: *const u8, len: u32, buf_index: u16) {
        unsafe {
            Self::push_entry(
                &mut self.uring.submission(),
                &io_uring::opcode::WriteFixed::new(fd, buf, len, buf_index)
                    .build()
                    .user_data((fd.0 as u64) | SEND_MARKER),
            );
        }
    }

    pub fn cancel(&mut self, cancel_builder: io_uring::types::CancelBuilder) {
        self.uring
            .submitter()
            .register_sync_cancel(None, cancel_builder)
            .unwrap();
    }

    /// To register new buffers, unregister must be called first
    /// # Safety
    /// buffers must be valid
    pub unsafe fn register_buffers(&mut self, buffers: &[iovec]) {
        // println!("registering buffers {:?}", buffers);
        self.uring.submitter().register_buffers(buffers).unwrap();
    }

    /// All requests in the submission queue must be finished or cancelled, or else this function
    /// will hang indefinetely.
    pub fn unregister_buffers(&mut self) {
        self.uring.submitter().unregister_buffers().unwrap();
    }
}
