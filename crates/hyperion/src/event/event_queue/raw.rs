use std::{
    any::TypeId, cell::SyncUnsafeCell, mem::MaybeUninit, ptr::NonNull, sync::atomic::AtomicUsize,
};

use anyhow::bail;

/// Denotes a pointer that will become invalid at the end of the tick (it is bump allocated)
#[derive(Debug, Copy, Clone)]
pub struct BumpPtr {
    id: TypeId,
    // a ptr to a bump allocated event
    elem: NonNull<()>,
}

unsafe impl Send for BumpPtr {}
unsafe impl Sync for BumpPtr {}

impl BumpPtr {
    pub const fn new(id: TypeId, elem: NonNull<()>) -> Self {
        Self { id, elem }
    }

    pub const fn id(&self) -> TypeId {
        self.id
    }

    pub const fn elem(&self) -> NonNull<()> {
        self.elem
    }
}

pub struct RawQueue {
    elems: Box<[SyncUnsafeCell<MaybeUninit<BumpPtr>>]>,
    len: AtomicUsize,
}

impl RawQueue {
    pub fn new(size: usize) -> Self {
        let elems = (0..size)
            .map(|_| SyncUnsafeCell::new(MaybeUninit::uninit()))
            .collect();

        Self {
            elems,
            len: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, elem: BumpPtr) -> anyhow::Result<()> {
        let ptr = self.len.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let elems = &*self.elems;

        let Some(ptr) = elems.get(ptr) else {
            self.len.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            bail!("queue is full");
        };

        let ptr = unsafe { &mut *ptr.get() };
        ptr.write(elem);

        Ok(())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = BumpPtr> + '_ {
        let elems = &mut *self.elems;
        let len = *self.len.get_mut();

        (0..len).map(move |i| {
            let elem = elems.get_mut(i).unwrap();
            let elem = elem.get_mut();
            let elem = unsafe { elem.assume_init_mut() };

            *elem
        })
    }

    pub fn reset(&mut self) {
        // we do not need to `Drop` because NonNull does not implement Drop
        *self.len.get_mut() = 0;
    }
}
