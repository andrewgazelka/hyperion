use std::{
    fmt::Debug,
    io::{Cursor, Write},
    mem::MaybeUninit,
};

use anyhow::ensure;
use tracing::trace;
use valence_protocol::{CompressionThreshold, Encode, Packet, VarInt};

use crate::{event::ScratchBuffer, net::MAX_PACKET_SIZE};

mod util;

pub struct PacketEncoder {
    threshold: CompressionThreshold,
}

impl Debug for PacketEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PacketEncoder")
            .field("threshold", &self.threshold)
            .finish()
    }
}

// todo:
// technically needs lifetimes to be write
// but ehhhh not doing this now we are referncing data which lives the duration of the program
// todo: bench if repr packed worth it (on old processors often slows down.
// Modern processors packed can actually be faster because cache locality)
#[allow(unused, reason = "this is used in linux")]
#[derive(Debug, Copy, Clone)]
#[repr(packed)]
pub struct DataWriteInfo {
    pub start_ptr: *const u8,
    pub len: u32,
}

unsafe impl Send for DataWriteInfo {}
unsafe impl Sync for DataWriteInfo {}

impl DataWriteInfo {
    pub const NULL: Self = Self {
        start_ptr: std::ptr::null(),
        len: 0,
    };

    /// # Safety
    /// todo
    #[allow(dead_code, reason = "nice for unit tests")]
    #[must_use]
    pub const unsafe fn as_slice(&self) -> &[u8] {
        std::slice::from_raw_parts(self.start_ptr, self.len as usize)
    }
}

/// Returns the number of bytes written to `buf`
pub fn append_packet_without_compression<P>(
    pkt: &P,
    buf: &mut [u8]
) -> anyhow::Result<usize>
where
    P: valence_protocol::Packet + Encode,
{
    let data_write_start = VarInt::MAX_SIZE as u64;

    let mut cursor = Cursor::new(buf);
    cursor.set_position(data_write_start);

    pkt.encode_with_id(&mut cursor)?;

    let data_len = cursor.position() as usize - data_write_start as usize;

    let packet_len_size = VarInt(data_len as i32).written_size();

    let packet_len = packet_len_size + data_len;
    ensure!(
        packet_len <= MAX_PACKET_SIZE,
        "packet exceeds maximum length"
    );

    let inner = cursor.into_inner();

    inner.copy_within(
        data_write_start as usize..data_write_start as usize + data_len,
        packet_len_size,
    );

    let mut cursor = Cursor::new(inner);
    VarInt(data_len as i32).encode(&mut cursor)?;

    trace!("without compression: {packet_len} bytes");

    Ok(packet_len)
}

impl PacketEncoder {
    #[must_use]
    pub const fn new(threshold: CompressionThreshold) -> Self {
        Self { threshold }
    }

    #[must_use]
    pub const fn compression_threshold(&self) -> CompressionThreshold {
        self.threshold
    }

    /// Returns the number of bytes written to `buf`
    pub fn append_packet_with_compression<P, B: Buf>(
        &self,
        pkt: &P,
        buf: &mut [u8],
        scratch: &mut impl ScratchBuffer,
        compressor: &mut libdeflater::Compressor,
    ) -> anyhow::Result<usize>
    where
        P: valence_protocol::Packet + Encode,
    {
        const DATA_LEN_0_SIZE: usize = 1;

        // + 1 because data len would be 0 if not compressed
        let data_write_start = (VarInt::MAX_SIZE + DATA_LEN_0_SIZE) as u64;

        let mut cursor = Cursor::new(buf);
        cursor.set_position(data_write_start);

        pkt.encode_with_id(&mut cursor)?;

        let end_data_position_exclusive = cursor.position();

        let data_len = end_data_position_exclusive - data_write_start;

        let threshold = u64::from(self.threshold.0.unsigned_abs());

        if data_len > threshold {
            let scratch = scratch.obtain();

            debug_assert!(scratch.is_empty());

            let data_slice =
                &mut buf[data_write_start as usize..end_data_position_exclusive as usize];

            {
                // todo: I think this kinda safe maybe??? ... lol. well I know at least scratch is always large enough
                let written = {
                    let scratch = scratch.spare_capacity_mut();
                    let scratch = unsafe { MaybeUninit::slice_assume_init_mut(scratch) };

                    compressor.zlib_compress(data_slice, scratch)?
                };

                unsafe {
                    scratch.set_len(scratch.len() + written);
                }
            }

            let data_len = VarInt(data_len as u32 as i32);

            let packet_len = data_len.written_size() + scratch.len();
            let packet_len = VarInt(packet_len as u32 as i32);

            let mut write = Cursor::new(buf);
            packet_len.encode(&mut write)?;
            data_len.encode(&mut write)?;
            write.write_all(scratch)?;

            let len = write.position();

            return Ok(buf.advance(len as usize));
        }

        let data_len_0 = VarInt(0);
        let packet_len = VarInt(DATA_LEN_0_SIZE as i32 + data_len as u32 as i32); // packet_len.written_size();

        let mut cursor = Cursor::new(buf);
        packet_len.encode(&mut cursor)?;
        data_len_0.encode(&mut cursor)?;

        let pos = cursor.position();

        buf.copy_within(
            data_write_start as usize..end_data_position_exclusive as usize,
            pos as usize,
        );

        let len = pos as u32 + (end_data_position_exclusive - data_write_start) as u32;

        Ok(buf.advance(len as usize))
    }

    pub fn append_packet<P, B: Buf>(
        &self,
        pkt: &P,
        buf: &mut B,
        scratch: &mut impl ScratchBuffer,
        compressor: &mut libdeflater::Compressor,
    ) -> anyhow::Result<usize>
    where
        P: Packet + Encode,
    {
        let has_compression = self.threshold.0 >= 0;

        if has_compression {
            self.append_packet_with_compression(pkt, buf, scratch, compressor)
        } else {
            append_packet_without_compression(pkt, buf)
        }
    }

    pub fn set_compression(&mut self, threshold: CompressionThreshold) {
        self.threshold = threshold;
    }
}

// I do not think these tests are valid anymore because libdeflater is not one-to-one compression with flate2 (zlib)
// #[cfg(test)]
// mod tests {
//     use bumpalo::Bump;
//     use libdeflater::CompressionLvl;
//     use valence_protocol::{
//         packets::login, Bounded, CompressionThreshold, Encode, Packet,
//         PacketEncoder as ValencePacketEncoder,
//     };
//
//     use crate::{
//         events::Scratch,
//         net::{encoder::PacketEncoder, MAX_PACKET_SIZE},
//         singleton::ring::Ring,
//     };
//
//     fn compare_pkt<P: Packet + Encode>(packet: &P, compression: CompressionThreshold, msg: &str) {
//         let mut large_ring = Ring::new(MAX_PACKET_SIZE * 2);
//
//         let mut encoder = PacketEncoder::new(compression, CompressionLvl::new(4).unwrap());
//
//         let bump = Bump::new();
//         let mut scratch = Scratch::from(&bump);
//         let encoder_res = encoder
//             .append_packet(packet, &mut large_ring, &mut scratch)
//             .unwrap();
//
//         let mut valence_encoder = ValencePacketEncoder::new();
//         valence_encoder.set_compression(compression);
//         valence_encoder.append_packet(packet).unwrap();
//
//         let encoder_res = unsafe { encoder_res.as_slice() };
//
//         let valence_encoder_res = valence_encoder.take().to_vec();
//
//         // to slice
//         let valence_encoder_res = valence_encoder_res.as_slice();
//
//         let encoder_res = hex::encode(encoder_res);
//         let valence_encoder_res = hex::encode(valence_encoder_res);
//
//         // add 0x
//         let encoder_res = format!("0x{encoder_res}");
//         let valence_encoder_res = format!("0x{valence_encoder_res}");
//
//         assert_eq!(encoder_res, valence_encoder_res, "{msg}");
//     }
//
//     fn compare_pkt2<P: Packet + Encode>(
//         packet1: &P,
//         packet2: &P,
//         compression: CompressionThreshold,
//         msg: &str,
//     ) {
//         let mut large_ring = Ring::new(MAX_PACKET_SIZE * 2);
//
//         let mut encoder = PacketEncoder::new(compression, CompressionLvl::new(4).unwrap());
//
//         let bump = Bump::new();
//         let mut scratch = Scratch::from(&bump);
//
//         let encoder_res1 = encoder
//             .append_packet(packet1, &mut large_ring, &mut scratch)
//             .unwrap();
//
//         let mut valence_encoder = ValencePacketEncoder::new();
//         valence_encoder.set_compression(compression);
//         valence_encoder.append_packet(packet1).unwrap();
//
//         let encoder_res2 = encoder
//             .append_packet(packet2, &mut large_ring, &mut scratch)
//             .unwrap();
//
//         println!("encoder_res1: {encoder_res1:?}");
//         let encoder_res1 = unsafe { encoder_res1.as_slice() };
//         println!("encoder_res1: {encoder_res1:X?}");
//
//         valence_encoder.append_packet(packet2).unwrap();
//
//         println!("encoder_res2: {encoder_res2:?}");
//         let encoder_res2 = unsafe { encoder_res2.as_slice() };
//         println!("encoder_res2: {encoder_res2:X?}");
//
//         let combined_res = encoder_res1
//             .iter()
//             .chain(encoder_res2)
//             .copied()
//             .collect::<Vec<u8>>();
//
//         let valence_encoder_res = valence_encoder.take().to_vec();
//
//         // to slice
//         let valence_encoder_res = valence_encoder_res.as_slice();
//
//         let encoder_res = hex::encode(combined_res);
//         let valence_encoder_res = hex::encode(valence_encoder_res);
//
//         // add 0x
//         let encoder_res = format!("0x{encoder_res}");
//         let valence_encoder_res = format!("0x{valence_encoder_res}");
//
//         assert_eq!(encoder_res, valence_encoder_res, "{msg}");
//     }
//
//     #[test]
//     fn test_uncompressed() {
//         fn compare<P: Packet + Encode>(packet: &P, msg: &str) {
//             compare_pkt(packet, CompressionThreshold::default(), msg);
//         }
//
//         let login = login::LoginHelloC2s {
//             username: Bounded::default(),
//             profile_id: None,
//         };
//         compare(&login, "Empty LoginHelloC2s");
//
//         let login = login::LoginHelloC2s {
//             username: Bounded("Emerald_Explorer"),
//             profile_id: None,
//         };
//         compare(&login, "LoginHelloC2s with 'Emerald_Explorer'");
//     }
//
//     #[test]
//     fn test_compressed2() {
//         fn compare<P: Packet + Encode>(packet1: &P, packet2: &P, msg: &str) {
//             compare_pkt2(packet1, packet2, CompressionThreshold(2), msg);
//         }
//
//         fn random_name(input: &mut String) {
//             let length = fastrand::usize(..14);
//             for _ in 0..length {
//                 let c = fastrand::alphanumeric();
//                 input.push(c);
//             }
//         }
//
//         fastrand::seed(7);
//
//         let mut name1 = String::new();
//         let mut name2 = String::new();
//         for idx in 0..1000 {
//             random_name(&mut name1);
//             random_name(&mut name2);
//
//             let pkt1 = login::LoginHelloC2s {
//                 username: Bounded(&name1),
//                 profile_id: None,
//             };
//
//             let pkt2 = login::LoginHelloC2s {
//                 username: Bounded(&name2),
//                 profile_id: None,
//             };
//
//             compare(
//                 &pkt1,
//                 &pkt2,
//                 &format!("LoginHelloC2s with '{name1}' and '{name2}' on idx {idx}"),
//             );
//
//             name1.clear();
//             name2.clear();
//         }
//     }
//
//     #[test]
//     fn test_compressed() {
//         fn compare<P: Packet + Encode>(packet: &P, msg: &str) {
//             compare_pkt(packet, CompressionThreshold(10), msg);
//         }
//
//         fn random_name(input: &mut String) {
//             let length = fastrand::usize(..14);
//             for _ in 0..length {
//                 let c = fastrand::alphanumeric();
//                 input.push(c);
//             }
//         }
//
//         let login = login::LoginHelloC2s {
//             username: Bounded::default(),
//             profile_id: None,
//         };
//         compare(&login, "Empty LoginHelloC2s");
//
//         let login = login::LoginHelloC2s {
//             username: Bounded("Emerald_Explorer"),
//             profile_id: None,
//         };
//         compare(&login, "LoginHelloC2s with 'Emerald_Explorer'");
//
//         fastrand::seed(7);
//
//         let mut name = String::new();
//         for _ in 0..1000 {
//             random_name(&mut name);
//
//             let pkt = login::LoginHelloC2s {
//                 username: Bounded(&name),
//                 profile_id: None,
//             };
//
//             compare(&pkt, &format!("LoginHelloC2s with '{name}'"));
//
//             name.clear();
//         }
//     }
//
//     #[test]
//     fn test_compressed_very_small_double() {
//         fn compare<P: Packet + Encode>(packet: &P, msg: &str) {
//             compare_pkt(packet, CompressionThreshold(2), msg);
//         }
//
//         fn random_name(input: &mut String) {
//             let length = fastrand::usize(..14);
//             for _ in 0..length {
//                 let c = fastrand::alphanumeric();
//                 input.push(c);
//             }
//         }
//
//         let login = login::LoginHelloC2s {
//             username: Bounded::default(),
//             profile_id: None,
//         };
//         compare(&login, "Empty LoginHelloC2s");
//
//         let login = login::LoginHelloC2s {
//             username: Bounded("Emerald_Explorer"),
//             profile_id: None,
//         };
//         compare(&login, "LoginHelloC2s with 'Emerald_Explorer'");
//
//         fastrand::seed(7);
//
//         let mut name = String::new();
//         for _ in 0..1000 {
//             random_name(&mut name);
//
//             let pkt = login::LoginHelloC2s {
//                 username: Bounded(&name),
//                 profile_id: None,
//             };
//
//             compare(&pkt, &format!("LoginHelloC2s with '{name}'"));
//
//             name.clear();
//         }
//     }
// }
