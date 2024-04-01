// https://stackoverflow.com/a/61681112/4889030
// https://matklad.github.io/2020/10/03/fast-thread-locals-in-rust.html
use std::cell::UnsafeCell;

use anyhow::{ensure, Context};
use bytes::{BufMut, Bytes};
use evenio::component::Component;
use valence_protocol::{Encode, Packet, VarInt};

#[derive(Default)]
struct ConstPacketEncoder {
    buf: Vec<u8>,
}

impl ConstPacketEncoder {
    pub const fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn append_packet<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: Packet + Encode,
    {
        let start_len = self.buf.len();

        pkt.encode_with_id((&mut self.buf).writer())?;

        let data_len = self.buf.len() - start_len;

        let packet_len = data_len;

        ensure!(
            packet_len <= valence_protocol::MAX_PACKET_SIZE as usize,
            "packet exceeds maximum length"
        );

        let packet_len = i32::try_from(packet_len).context("packet length exceeds i32")?;

        let packet_len_size = VarInt(packet_len).written_size();

        self.buf.put_bytes(0, packet_len_size);
        self.buf
            .copy_within(start_len..start_len + data_len, start_len + packet_len_size);

        #[expect(
            clippy::indexing_slicing,
            reason = "we are only growing buf, and its original length is start_len, so this is a \
                      valid operation"
        )]
        let front = &mut self.buf[start_len..];

        VarInt(packet_len).encode(front)?;

        Ok(())
    }
}

#[thread_local]
static ENCODER: UnsafeCell<ConstPacketEncoder> = UnsafeCell::new(ConstPacketEncoder::new());

pub struct Encoder;

impl Encoder {
    #[expect(clippy::unused_self)]
    pub fn append<P: Packet + Encode>(packet: &P) -> anyhow::Result<()> {
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
        let encoder = unsafe { &mut *ENCODER.get() };
        encoder.append_packet(packet)
    }

    #[allow(clippy::unused_self)]
    pub fn par_drain<F>(&self, f: F)
    where
        F: Fn(Bytes) + Sync,
    {
        rayon::broadcast(move |_| {
            // Safety:
            // ditto
            let encoder = unsafe { &mut *ENCODER.get() };
            let buf = core::mem::take(&mut encoder.buf);
            f(Bytes::from(buf));
        });
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
