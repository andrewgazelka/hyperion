use std::{
    alloc::{Allocator, Global, Layout},
    mem::{ManuallyDrop, MaybeUninit},
    ptr,
    ptr::NonNull,
    sync::atomic::{AtomicU32, Ordering},
};

pub struct Queue<T, A: Allocator = Global> {
    data: Box<[MaybeUninit<T>], A>,
    head: AtomicU32,
    alloc: A,
}

impl<T> Queue<T, Global> {
    pub fn new(cap: usize) -> Self {
        Self::new_in(cap, Global)
    }
}

impl<T, A: Allocator> Queue<T, A> {
    pub fn new_in(cap: usize, alloc: A) -> Self {
        let data = box_uninit(cap, &alloc);
        let head = AtomicU32::new(0);

        Self { data, head, alloc }
    }

    pub fn push(&self, value: T) -> u32 {
        // right ordering?
        let head_idx = self.head.fetch_add(1, Ordering::Relaxed);

        // ultimately, is this safe?
        let head = self.data[head_idx as usize].as_ptr().cast_mut();

        unsafe {
            ptr::write(head, value);
        }

        head_idx
    }

    #[allow(clippy::as_ptr_cast_mut)]
    pub fn into_inner(self) -> Box<[T], A> {
        let len = self.head.load(Ordering::Relaxed) as usize;
        let mut data = ManuallyDrop::new(self.data);

        unsafe {
            let slice = ptr::slice_from_raw_parts_mut(data.as_mut_ptr() as *mut T, len);
            Box::from_raw_in(slice, ptr::read(&self.alloc))
        }
    }
}

fn box_uninit<T, A: Allocator>(cap: usize, alloc: &A) -> Box<[MaybeUninit<T>], A> {
    let layout = Layout::array::<T>(cap).unwrap();
    let res = alloc.allocate(layout).unwrap();

    let res: NonNull<T> = res.cast();

    unsafe {
        let slice = ptr::slice_from_raw_parts_mut(res.as_ptr() as *mut MaybeUninit<T>, cap);
        Box::from_raw_in(slice, ptr::read(alloc))
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
