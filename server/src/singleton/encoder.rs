// https://stackoverflow.com/a/61681112/4889030
// https://matklad.github.io/2020/10/03/fast-thread-locals-in-rust.html
use std::cell::UnsafeCell;

use evenio::component::Component;
use thread_local::ThreadLocal;
use valence_protocol::{Encode, Packet, PacketEncoder};

#[derive(Default, Component)]
pub struct Encoder {
    local: ThreadLocal<UnsafeCell<PacketEncoder>>,
}

impl Encoder {
    pub fn append<P: Packet + Encode>(&self, packet: &P) -> anyhow::Result<()> {
        let encoder = self.local.get_or_default();

        // Safety:
        // The use of `unsafe` here is justified by the guarantees provided by the `ThreadLocal` and
        // `UnsafeCell` usage patterns:
        // 1. Thread-local storage ensures that the `UnsafeCell<PacketEncoder>` is accessed only
        //    within the context of a single thread, eliminating the risk of concurrent access
        //    violations.
        // 2. `UnsafeCell` is the fundamental building block for mutable shared state in Rust. By
        //    using `UnsafeCell`, we're explicitly signaling that the contained value
        //    (`PacketEncoder`) may be mutated through a shared reference. This is necessary because
        //    Rust's borrowing rules disallow mutable aliasing, which would be violated if we
        //    attempted to mutate through a shared reference without `UnsafeCell`.
        // 3. The dereference of `encoder.get()` to obtain a mutable reference to the
        //    `PacketEncoder` (`&mut *encoder.get()`) is safe under the assumption that no other
        //    references to the `PacketEncoder` are concurrently alive. This assumption is upheld by
        //    the `ThreadLocal` storage, ensuring that the mutable reference is exclusive to the
        //    current thread.
        // Therefore, the use of `unsafe` is encapsulated within this method and does not leak
        // unsafe guarantees to the caller, provided the `Encoder` struct itself is used in a
        // thread-safe manner.
        let encoder = unsafe { &mut *encoder.get() };
        encoder.append_packet(packet)
    }

    pub fn drain(&mut self) -> impl Iterator<Item = bytes::Bytes> + '_ {
        self.local.iter_mut().map(|encoder| {
            let encoder = encoder.get_mut();
            encoder.take().freeze()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{RefUnwindSafe, UnwindSafe};

    use crate::singleton::encoder::Encoder;

    const fn _assert_auto_trait_impls()
    where
        Encoder: Send + Sync + UnwindSafe + RefUnwindSafe,
    {
    }
}
