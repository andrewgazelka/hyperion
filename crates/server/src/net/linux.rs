//! All the networking related code.

use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout},
    cmp,
    iter::TrustedLen,
    marker::PhantomData,
    net::{SocketAddr, ToSocketAddrs},
    os::fd::AsRawFd,
    sync::atomic::{AtomicU16, Ordering},
};

pub use io_uring::types::Fixed;
use io_uring::{
    cqueue::buffer_select, squeue, squeue::SubmissionQueue, types::BufRingEntry, IoUring,
};
use libc::iovec;
use socket2::Socket;
use tracing::{error, info, instrument, trace, warn};

use super::WriteItem;
use crate::net::{Fd, ServerDef, ServerEvent};

const COMPLETION_QUEUE_SIZE: u32 = 32768;
const SUBMISSION_QUEUE_SIZE: u32 = 32768;
const IO_URING_FILE_COUNT: u32 = 32768;
const C2S_RING_BUFFER_COUNT: usize = 16384;
const LISTEN_BACKLOG: libc::c_int = 128;
// const SEND_BUFFER_SIZE: usize = 128 * 1024 * 1024;

/// Size of each buffer in bytes
const C2S_RING_BUFFER_LEN: usize = 64;

const LISTENER_FIXED_FD: Fixed = Fixed(0);
const C2S_BUFFER_GROUP_ID: u16 = 0;

const IORING_CQE_F_MORE: u32 = 1 << 1;

fn page_size() -> usize {
    // SAFETY: This is valid
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    usize::try_from(page_size).expect("page size is too large")
}

struct PageAlignedMemory<T> {
    data: *mut T,
    layout: Layout,
}

impl<T> PageAlignedMemory<T> {
    fn from_iter(iter: impl TrustedLen<Item = T>) -> Self {
        let len = iter
            .size_hint()
            .1
            .expect("iterator doesn't have a known size");
        assert!(len > 0);
        let page_size = page_size();
        let type_layout = Layout::new::<T>();
        assert!(type_layout.align() <= page_size);
        assert!(type_layout.size() > 0);

        let layout = Layout::from_size_align(
            type_layout
                .size()
                .checked_mul(len)
                .expect("allocation size is too large"),
            page_size,
        )
        .unwrap();

        // SAFETY: len is nonzero and T is not zero sized
        let data = unsafe { alloc(layout) };

        if data.is_null() {
            handle_alloc_error(layout);
        }

        let data: *mut T = data.cast();

        // Initialize the memory
        for (index, value) in iter.enumerate() {
            // SAFETY: index is guaranteed to be within bounds because TrustedLen guarantees
            // correctness and the amount of data from TrustedLen was allocated
            let value_ptr = unsafe { data.add(index) };
            // SAFETY: value_ptr is valid
            unsafe {
                value_ptr.write(value);
            }
        }

        Self { data, layout }
    }

    #[expect(dead_code, reason = "this is not used")]
    const fn as_mut_ptr(&self) -> *mut T {
        self.data
    }
}

impl<T> Drop for PageAlignedMemory<T> {
    fn drop(&mut self) {
        // SAFETY: data and layout should be valid
        unsafe {
            dealloc(self.data.cast(), self.layout);
        }
    }
}

pub struct LinuxServer {
    #[expect(dead_code, reason = "this is used so there is no drop")]
    listener: Socket,

    uring: IoUring,

    /// The underlying data should never be accessed directly as a slice because the kernel could modify some
    /// buffers while a reference to the data is held, which would cause undefined behavior. In
    /// addition, this field must be declared after uring so that the uring is dropped first. This
    /// is needed because registered buffers must be valid until unregistered or the uring is dropped.
    c2s_buffer: Vec<[u8; C2S_RING_BUFFER_LEN]>,

    /// This field must be declared after uring so that the uring is dropped first. This
    /// is needed because registered buffers must be valid until unregistered or the uring is dropped.
    c2s_buffer_entries: PageAlignedMemory<BufRingEntry>,

    /// Value of `c2s_buffer_entries` tail, which is synched occasionally with the kernel
    c2s_local_tail: u16,

    pending_writes: usize,

    /// Make Listener !Send and !Sync to let `io_uring` assume that it'll only be accessed by 1
    /// thread
    phantom: PhantomData<*const ()>,
}

impl ServerDef for LinuxServer {
    fn new(address: SocketAddr) -> anyhow::Result<Self> {
        let Some(address) = address.to_socket_addrs()?.next() else {
            anyhow::bail!("no addresses specified")
        };
        let domain = match address {
            SocketAddr::V4(_) => socket2::Domain::IPV4,
            SocketAddr::V6(_) => socket2::Domain::IPV6,
        };

        let listener = Socket::new(domain, socket2::Type::STREAM, None)?;
        listener.set_nonblocking(true)?;
        // listener.set_send_buffer_size(SEND_BUFFER_SIZE)?;
        listener.bind(&address.into())?;
        listener.listen(LISTEN_BACKLOG)?;

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
        let c2s_buffer = vec![[0u8; C2S_RING_BUFFER_LEN]; C2S_RING_BUFFER_COUNT];
        let c2s_buffer_entries = PageAlignedMemory::from_iter(c2s_buffer.iter().enumerate().map(
            |(buffer_id, buffer)| {
                // SAFETY: BufRingEntry is valid in the all-zero byte-pattern.
                let mut entry = unsafe { std::mem::zeroed::<BufRingEntry>() };
                entry.set_addr(buffer.as_ptr() as u64);
                entry.set_len(buffer.len() as u32);
                entry.set_bid(buffer_id as u16);
                entry
            },
        ));

        let tail = C2S_RING_BUFFER_COUNT as u16;

        // Update the tail
        // SAFETY: This is the first entry of the buffer ring
        let tail_addr = unsafe { BufRingEntry::tail(c2s_buffer_entries.data) };

        // SAFETY: tail_addr can be set without an atomic since it hasn't been passed to the kernel
        // yet
        unsafe {
            *tail_addr.cast_mut() = tail;
        }

        // Register the buffer ring
        // SAFETY: c2s_buffer_entries is valid to write to for C2S_RING_BUFFER_COUNT BufRingEntry structs
        unsafe {
            submitter.register_buf_ring(
                c2s_buffer_entries.data as u64,
                C2S_RING_BUFFER_COUNT as u16,
                C2S_BUFFER_GROUP_ID,
            )?;
        }

        Self::request_accept(&mut uring.submission());

        Ok(Self {
            listener,
            uring,
            c2s_buffer,
            c2s_buffer_entries,
            c2s_local_tail: tail,
            pending_writes: 0,
            phantom: PhantomData,
        })
    }

    /// `f` should never panic
    #[instrument(skip_all, level = "trace", name = "iou-drain-events")]
    fn drain(&mut self, mut f: impl FnMut(ServerEvent)) -> std::io::Result<()> {
        let (_submitter, mut submission, mut completion) = self.uring.split();
        completion.sync();
        if completion.overflow() > 0 {
            error!(
                "the io_uring completion queue overflowed, and some connection errors are likely \
                 to occur; consider increasing COMPLETION_QUEUE_SIZE to avoid this"
            );
        }

        for event in completion {
            let result = event.result();
            match event.user_data() {
                0 => {
                    if event.flags() & IORING_CQE_F_MORE == 0 {
                        warn!("multishot accept rerequested");
                        Self::request_accept(&mut submission);
                    }

                    if result < 0 {
                        error!("there was an error in accept: {}", result);
                        continue;
                    }

                    #[expect(clippy::cast_sign_loss, reason = "we are checking if < 0")]
                    let fd = Fixed(result as u32);
                    Self::request_recv(&mut submission, fd);
                    f(ServerEvent::AddPlayer { fd: Fd(fd) });
                }
                1 => {
                    if result < 0 {
                        error!("there was an error in socket close: {}", result);
                    }
                }
                write if write & SEND_MARKER != 0 => {
                    let fd = Fixed((write & !SEND_MARKER) as u32);

                    self.pending_writes -= 1;

                    match result.cmp(&0) {
                        cmp::Ordering::Less => {
                            error!("there was an error in write: {}", result);
                            // Nothing is done here. It's assumed that if there is a write error,
                            // read will error too, and all of the error handling occurs in read.
                            // This code intentionally does not shutdown nor close the socket
                            // because read may close the socket before this does, and if this code
                            // closes the socket afterwards, it could close another player's
                            // socket.
                        }
                        cmp::Ordering::Equal => {
                            // This should never happen as long as write is never passed an empty buffer:
                            // https://stackoverflow.com/questions/5656628/what-should-i-do-when-writefd-buf-count-returns-0
                            unreachable!("write returned 0 which should not be possible");
                        }
                        cmp::Ordering::Greater => {
                            // Write operation completed successfully
                            // TODO: Check that write wasn't truncated
                            trace!("successful write response");

                            f(ServerEvent::SentData { fd: Fd(fd) });
                        }
                    }
                }
                read if read & RECV_MARKER != 0 => {
                    let fd = Fixed((read & !RECV_MARKER) as u32);
                    let more = event.flags() & IORING_CQE_F_MORE != 0;

                    if result == -libc::ECONNRESET || result == -libc::ETIMEDOUT || result == 0 {
                        trace!("player {fd:?} disconnected during recv (code {result})");

                        assert!(
                            !more,
                            "errors and EOF should result in no longer reading the socket. this \
                             check is needed to avoid removing the same player multiple times"
                        );

                        f(ServerEvent::RemovePlayer { fd: Fd(fd) });
                        Self::close(&mut submission, fd);
                    } else {
                        // The player is not getting disconnected, but there still may be errors

                        if !more {
                            // No more completion events will occur from this multishot recv. This
                            // will need to request another multishot recv.
                            warn!("socket recv rerequested");
                            Self::request_recv(&mut submission, fd);
                        }

                        if result > 0 {
                            #[expect(clippy::cast_sign_loss, reason = "we are checking if < 0")]
                            let bytes_received = result as usize;
                            let buffer_id =
                                buffer_select(event.flags()).expect("there should be a buffer");
                            assert!((buffer_id as usize) < C2S_RING_BUFFER_COUNT);
                            // SAFETY: as_mut_ptr doesn't take a reference to the slice in c2s_buffer.
                            // buffer_id is in bounds of c2s_buffer, so all the
                            // safety requirements for add is met.
                            let buffer_ptr =
                                unsafe { self.c2s_buffer.as_mut_ptr().add(buffer_id as usize) };
                            // SAFETY: buffer_id is in bounds, so buffer_ptr is valid
                            let buffer = unsafe { &(*buffer_ptr)[..bytes_received] };
                            self.c2s_local_tail = self.c2s_local_tail.wrapping_add(1);
                            f(ServerEvent::RecvData {
                                fd: Fd(fd),
                                data: buffer,
                            });
                        } else if result == -libc::ENOBUFS {
                            warn!(
                                "ran out of c2s buffers which will negatively impact performance; \
                                 consider increasing C2S_RING_BUFFER_COUNT"
                            );
                        } else {
                            error!("unhandled recv error: {result}");
                        }
                    }
                }
                _ => {
                    panic!("unexpected event: {event:?}");
                }
            }
        }

        // SAFETY: This is the first entry of the buffer ring
        let tail_addr = unsafe { BufRingEntry::tail(self.c2s_buffer_entries.data) };
        // Casting it into an atomic is needed since the kernel is also reading the tail
        let tail_addr: *const AtomicU16 = tail_addr.cast();
        // SAFETY: tail_addr is valid
        unsafe {
            (*tail_addr).store(self.c2s_local_tail, Ordering::Relaxed);
        }

        Ok(())
    }

    #[instrument(skip_all, level = "trace", name = "iou-register-buffers")]
    unsafe fn register_buffers(&mut self, buffers: &[iovec]) {
        info!("registering buffers");
        unsafe { self.register_buffers(buffers) };
        info!("finished registering buffers");
    }

    /// Impl with local sends BEFORE broadcasting
    // #[instrument(skip_all, level = "trace")]
    fn write(&mut self, item: WriteItem) {
        let WriteItem {
            info,
            buffer_idx,
            fd,
        } = item;

        let fd = fd.0;
        self.write_raw(fd, info.start_ptr, info.len, buffer_idx);
    }

    #[instrument(skip_all, level = "trace", name = "iou-submit-events")]
    fn submit_events(&mut self) {
        if let Err(err) = self.uring.submit() {
            error!("unexpected io_uring error during submit: {err}");
        }
    }
}

const RECV_MARKER: u64 = 0b1 << 63;
const SEND_MARKER: u64 = 0b1 << 62;

impl LinuxServer {
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
                    .user_data(u64::from(fd.0) | RECV_MARKER),
            );
        }
    }

    /// Calling `close` on the same fd should not be done multiple times to avoid shutting down
    /// other fds that may take the place of the fd that was closed.
    fn close(submission: &mut SubmissionQueue, fd: Fixed) {
        unsafe {
            Self::push_entry(
                submission,
                &io_uring::opcode::Close::new(fd).build().user_data(1),
            );
        }
    }

    pub fn write_raw(&mut self, fd: Fixed, buf: *const u8, len: u32, buf_index: u16) {
        self.pending_writes += 1;
        unsafe {
            Self::push_entry(
                &mut self.uring.submission(),
                &io_uring::opcode::WriteFixed::new(fd, buf, len, buf_index)
                    .build()
                    // IO_HARDLINK allows adjacent fd writes to be sequential which is SUPER important to make
                    // sure things get written in the right (or at least deterministic) order
                    .flags(squeue::Flags::IO_HARDLINK)
                    .user_data(u64::from(fd.0) | SEND_MARKER),
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
        self.uring.submitter().register_buffers(buffers).unwrap();
    }

    /// All requests in the submission queue must be finished or cancelled, or else this function
    /// will hang indefinetely.
    pub fn unregister_buffers(&mut self) {
        self.uring.submitter().unregister_buffers().unwrap();
    }
}
