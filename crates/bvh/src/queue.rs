// https://doc.rust-lang.org/nomicon/vec/vec.html

use std::{
    alloc::{Allocator, Global, Layout},
    mem,
    mem::ManuallyDrop,
    ptr,
    ptr::NonNull,
    sync::atomic::{AtomicU32, Ordering},
};

pub struct Queue<T, A: Allocator = Global> {
    ptr: NonNull<T>,
    head: AtomicU32,
    alloc: A,
    capacity: usize,
}

unsafe impl<T: Send> Send for Queue<T> {}
unsafe impl<T: Sync> Sync for Queue<T> {}

impl<T> Queue<T, Global> {
    pub fn new(cap: usize) -> Self {
        Self::new_in(cap, Global)
    }
}

impl<T, A: Allocator> Drop for Queue<T, A> {
    fn drop(&mut self) {
        let pop = || {
            let head = self.head.load(Ordering::Relaxed);
            if head == 0 {
                return None;
            }

            let head = head - 1;
            self.head.store(head, Ordering::Relaxed);

            let head = self.ptr.as_ptr().wrapping_offset(head as isize);

            unsafe { Some(ptr::read(head)) }
        };

        while pop().is_some() {}

        let layout = Layout::array::<T>(self.capacity).unwrap();
        unsafe {
            self.alloc.deallocate(self.ptr.cast(), layout);
        }
    }
}

impl<T, A: Allocator> Queue<T, A> {
    pub fn new_in(cap: usize, alloc: A) -> Self {
        assert!(mem::size_of::<T>() != 0, "We do not handle ZSTs");
        assert!(cap != 0, "Queue must have a capacity of at least 1");
        let layout = Layout::array::<T>(cap).unwrap();
        let data = alloc.allocate(layout).unwrap();

        let data: NonNull<T> = data.cast();

        Self {
            ptr: data,
            head: AtomicU32::new(0),
            alloc,
            capacity: cap,
        }
    }

    pub fn push(&self, value: T) -> u32 {
        // right ordering?
        let head_idx = self.head.fetch_add(1, Ordering::Relaxed);

        if head_idx as usize >= self.capacity {
            panic!("Queue is full");
        }

        // ultimately, is this safe?
        let head = self.ptr.as_ptr().wrapping_offset(head_idx as isize);

        unsafe {
            ptr::write(head, value);
        }

        head_idx
        // 1
    }

    pub fn into_inner(self) -> Vec<T, A> {
        let len = self.head.load(Ordering::Relaxed) as usize;

        let me = ManuallyDrop::new(self);

        let data = me.ptr;
        let alloc = unsafe { ptr::read(&me.alloc) };

        unsafe { Vec::from_raw_parts_in(data.as_ptr(), len, me.capacity, alloc) }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_queue() {
        let queue = super::Queue::new(10);

        let idx = queue.push(1);
        assert_eq!(idx, 0);
        let idx = queue.push(2);
        assert_eq!(idx, 1);
        let idx = queue.push(3);
        assert_eq!(idx, 2);

        let inner = queue.into_inner();
        assert_eq!(inner[0], 1);
        assert_eq!(inner[1], 2);
        assert_eq!(inner[2], 3);
    }
}
