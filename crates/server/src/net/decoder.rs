use anyhow::{bail, ensure, Context};
use bytes::{Buf, BytesMut};
use flecs_ecs::macros::Component;
use more_asserts::debug_assert_ge;
use valence_protocol::{
    decode::PacketFrame, var_int::VarIntDecodeError, CompressionThreshold, Decode, VarInt,
    MAX_PACKET_SIZE,
};

use crate::ScratchBuffer;

#[derive(Default, Component)]
pub struct PacketDecoder {
    buf: BytesMut,
    threshold: CompressionThreshold,
}

impl PacketDecoder {
    #[allow(dead_code, reason = "this might be used in the future")]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn try_next_packet(
        &mut self,
        scratch: &mut impl ScratchBuffer,
    ) -> anyhow::Result<Option<PacketFrame>> {
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

                let decompression_buf = scratch.obtain();

                debug_assert!(decompression_buf.is_empty());
                debug_assert_ge!(decompression_buf.capacity(), MAX_PACKET_SIZE as usize);

                // decompression_buf.put_bytes(0, data_len as usize);

                // valid because scratch is always large enough
                unsafe { decompression_buf.set_len(data_len as usize) };

                let written_len = {
                    // todo: does it make sense to cache ever?
                    let mut decompressor = libdeflater::Decompressor::new();

                    decompressor.zlib_decompress(r, decompression_buf)?
                };

                debug_assert_eq!(written_len, data_len as usize);

                let total_packet_len = VarInt(packet_len).written_size() + packet_len as usize;

                self.buf.advance(total_packet_len);

                data = BytesMut::from(decompression_buf.as_slice());
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

        Ok(Some(PacketFrame {
            id: packet_id,
            body: data,
        }))
    }

    #[must_use]
    pub const fn compression(&self) -> CompressionThreshold {
        self.threshold
    }

    pub fn set_compression(&mut self, threshold: CompressionThreshold) {
        self.threshold = threshold;
    }

    pub fn queue_bytes(&mut self, bytes: BytesMut) {
        self.buf.unsplit(bytes);
    }

    pub fn queue_slice(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn take_capacity(&mut self) -> BytesMut {
        self.buf.split_off(self.buf.len())
    }

    pub fn reserve(&mut self, additional: usize) {
        self.buf.reserve(additional);
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

    fn compare_decoder(packet: &LoginHelloC2s, threshold: CompressionThreshold, msg: &str) {
        let mut valence_decoder = valence_protocol::PacketDecoder::new();
        valence_decoder.set_compression(threshold);

        let mut custom_decoder = PacketDecoder::new();
        custom_decoder.set_compression(threshold);

        let mut encoder = valence_protocol::PacketEncoder::new();
        encoder.set_compression(threshold);

        encoder.append_packet(packet).unwrap();
        let encoded_bytes = encoder.take();

        valence_decoder.queue_slice(&encoded_bytes);
        custom_decoder.queue_slice(&encoded_bytes);

        let mut scratch = Scratch::new();

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

        let valence_decoded_pkt: LoginHelloC2s = valence_frame.decode().unwrap();
        let custom_decoded_pkt: LoginHelloC2s = custom_frame.decode().unwrap();

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
        fn compare(packet: &LoginHelloC2s, msg: &str) {
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
