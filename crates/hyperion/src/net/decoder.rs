use std::{
    cell::Cell,
    ops::{Index, RangeFull},
};

use anyhow::{bail, ensure, Context};
use bytes::Buf;
use flecs_ecs::macros::Component;
use valence_protocol::{
    var_int::VarIntDecodeError, CompressionThreshold, Decode, Packet, VarInt, MAX_PACKET_SIZE,
};

use crate::ScratchBuffer;

#[derive(Default)]
struct RefBytesMut {
    cursor: Cell<usize>,
    inner: Vec<u8>,
}

impl RefBytesMut {
    pub fn advance(&self, amount: usize) {
        let on = self.cursor.get();
        self.cursor.set(on + amount);
    }

    pub fn split_to(&self, len: usize) -> &[u8] {
        let before = self.cursor.get();
        let after = before + len;
        self.cursor.set(after);
        &self.inner[before..after]
    }
}

unsafe impl Sync for RefBytesMut {}
unsafe impl Send for RefBytesMut {}

impl Index<RangeFull> for RefBytesMut {
    type Output = [u8];

    fn index(&self, index: RangeFull) -> &Self::Output {
        let on = self.cursor.get();
        &self.inner[on..]
    }
}

/// A buffer for saving bytes that are not yet decoded.
#[derive(Default, Component)]
pub struct PacketDecoder {
    buf: RefBytesMut,
    threshold: CompressionThreshold,
}

#[derive(Copy, Clone, Debug)]
pub struct BorrowedPacketFrame<'a> {
    /// The ID of the decoded packet.
    pub id: i32,
    /// The contents of the packet after the leading VarInt ID.
    pub body: &'a [u8],
}

impl<'a> BorrowedPacketFrame<'a> {
    /// Attempts to decode this packet as type `P`. An error is returned if the
    /// packet ID does not match, the body of the packet failed to decode, or
    /// some input was missed.
    pub fn decode<P>(&self) -> anyhow::Result<P>
    where
        P: Packet + Decode<'a>,
    {
        ensure!(
            P::ID == self.id,
            "packet ID mismatch while decoding '{}': expected {}, got {}",
            P::NAME,
            P::ID,
            self.id
        );

        let mut r = self.body;

        let pkt = P::decode(&mut r)?;

        ensure!(
            r.is_empty(),
            "missed {} bytes while decoding '{}'",
            r.len(),
            P::NAME
        );

        Ok(pkt)
    }
}

impl PacketDecoder {
    /// Tries to get the next packet from the buffer.
    /// If a new packet is found, the buffer will be truncated by the length of the packet.
    pub fn try_next_packet<'b>(
        &'b mut self,
        bump: &'b bumpalo::Bump,
    ) -> anyhow::Result<Option<BorrowedPacketFrame<'static>>> {
        let mut r = &self.buf[..];

        let packet_len = match VarInt::decode_partial(&mut r) {
            Ok(len) => len,
            Err(VarIntDecodeError::Incomplete) => return Ok(None),
            Err(VarIntDecodeError::TooLarge) => bail!("malformed packet length VarInt"),
        };

        ensure!(
            (0..=MAX_PACKET_SIZE).contains(&packet_len),
            "packet length of {packet_len} is out of bounds"
        );

        #[expect(clippy::cast_sign_loss, reason = "we are checking if < 0")]
        if r.len() < packet_len as usize {
            // Not enough data arrived yet.
            return Ok(None);
        }

        let packet_len_len = VarInt(packet_len).written_size();

        let mut data;

        #[expect(clippy::cast_sign_loss, reason = "we are checking if < 0")]
        if self.threshold.0 >= 0 {
            r = &r[..packet_len as usize];

            let data_len = VarInt::decode(&mut r)?.0;

            ensure!(
                (0..MAX_PACKET_SIZE).contains(&data_len),
                "decompressed packet length of {data_len} is out of bounds"
            );

            // Is this packet compressed?
            if data_len > 0 {
                ensure!(
                    data_len > self.threshold.0,
                    "decompressed packet length of {data_len} is <= the compression threshold of \
                     {}",
                    self.threshold.0
                );

                // todo(perf): make uninit memory ...  MaybeUninit
                let decompression_buf: &mut [u8] = bump.alloc_slice_fill_default(data_len as usize);

                let written_len = {
                    // todo: does it make sense to cache ever?
                    let mut decompressor = libdeflater::Decompressor::new();

                    decompressor.zlib_decompress(r, decompression_buf)?
                };

                debug_assert_eq!(written_len, data_len as usize);

                let total_packet_len = VarInt(packet_len).written_size() + packet_len as usize;

                self.buf.advance(total_packet_len);

                data = &*decompression_buf
            } else {
                debug_assert_eq!(data_len, 0);

                ensure!(
                    r.len() <= self.threshold.0 as usize,
                    "uncompressed packet length of {} exceeds compression threshold of {}",
                    r.len(),
                    self.threshold.0
                );

                let remaining_len = r.len();

                self.buf.advance(packet_len_len + 1);

                data = self.buf.split_to(remaining_len);
            }
        } else {
            self.buf.advance(packet_len_len);
            data = self.buf.split_to(packet_len as usize);
        }

        // Decode the leading packet ID.
        r = &data[..];
        let packet_id = VarInt::decode(&mut r)
            .context("failed to decode packet ID")?
            .0;

        data.advance(data.len() - r.len());

        let data = data.to_vec().into_boxed_slice();
        let data = Box::leak(data);

        Ok(Some(BorrowedPacketFrame {
            id: packet_id,
            body: data,
        }))
    }

    /// Get the compression threshold.
    #[must_use]
    pub const fn compression(&self) -> CompressionThreshold {
        self.threshold
    }

    /// Sets the compression threshold.
    pub fn set_compression(&mut self, threshold: CompressionThreshold) {
        self.threshold = threshold;
    }

    /// Queues a slice of bytes into the buffer.
    pub fn queue_slice(&mut self, bytes: &[u8]) {
        self.buf.inner.extend_from_slice(bytes);
    }
}

#[cfg(test)]
mod tests {
    use valence_protocol::{
        packets::{login, login::LoginHelloC2s},
        Bounded, CompressionThreshold,
    };

    use super::*;
    use crate::Scratch;

    fn compare_decoder(packet: &LoginHelloC2s<'_>, threshold: CompressionThreshold, msg: &str) {
        let mut valence_decoder = valence_protocol::PacketDecoder::new();
        valence_decoder.set_compression(threshold);

        let mut custom_decoder = PacketDecoder::default();
        custom_decoder.set_compression(threshold);

        let mut encoder = valence_protocol::PacketEncoder::new();
        encoder.set_compression(threshold);

        encoder.append_packet(packet).unwrap();
        let encoded_bytes = encoder.take();

        valence_decoder.queue_slice(&encoded_bytes);
        custom_decoder.queue_slice(&encoded_bytes);

        let mut scratch = Scratch::default();

        let valence_result = valence_decoder.try_next_packet().unwrap();
        let custom_result = custom_decoder.try_next_packet(&mut scratch).unwrap();

        assert_eq!(
            valence_result.is_some(),
            custom_result.is_some(),
            "Packet presence mismatch for {msg}"
        );

        let valence_frame = valence_result.unwrap();
        let custom_frame = custom_result.unwrap();

        assert_eq!(
            valence_frame.id, custom_frame.id,
            "Packet ID mismatch for {msg}"
        );
        assert_eq!(
            valence_frame.body, custom_frame.body,
            "Packet body mismatch for {msg}"
        );

        let valence_decoded_pkt: LoginHelloC2s<'_> = valence_frame.decode().unwrap();
        let custom_decoded_pkt: LoginHelloC2s<'_> = custom_frame.decode().unwrap();

        assert_eq!(
            valence_decoded_pkt.profile_id,
            custom_decoded_pkt.profile_id
        );
        assert_eq!(valence_decoded_pkt.username, custom_decoded_pkt.username);
    }

    #[test]
    fn test_uncompressed() {
        let login = login::LoginHelloC2s {
            username: Bounded::default(),
            profile_id: None,
        };
        compare_decoder(
            &login,
            CompressionThreshold::default(),
            "Empty LoginHelloC2s",
        );

        let login = login::LoginHelloC2s {
            username: Bounded("Emerald_Explorer"),
            profile_id: None,
        };
        compare_decoder(
            &login,
            CompressionThreshold::default(),
            "LoginHelloC2s with 'Emerald_Explorer'",
        );
    }

    #[test]
    fn test_compressed() {
        fn compare(packet: &LoginHelloC2s<'_>, msg: &str) {
            compare_decoder(packet, CompressionThreshold(10), msg);
        }

        fn random_name(input: &mut String) {
            let length = fastrand::usize(..14);
            for _ in 0..length {
                let c = fastrand::alphanumeric();
                input.push(c);
            }
        }

        let login = login::LoginHelloC2s {
            username: Bounded::default(),
            profile_id: None,
        };
        compare(&login, "Empty LoginHelloC2s");

        let login = login::LoginHelloC2s {
            username: Bounded("Emerald_Explorer"),
            profile_id: None,
        };
        compare(&login, "LoginHelloC2s with 'Emerald_Explorer'");

        fastrand::seed(7);

        let mut name = String::new();
        for _ in 0..1000 {
            random_name(&mut name);

            let pkt = login::LoginHelloC2s {
                username: Bounded(&name),
                profile_id: None,
            };

            compare(&pkt, &format!("LoginHelloC2s with '{name}'"));

            name.clear();
        }
    }

    // #[test]
    // fn test_compressed() {
    //     // ... similar tests as in the encoder, but using compare_decoder ...
    // }
}
