use std::io::{Cursor, Read};

use anyhow::ensure;
use tracing::{info, trace};
use valence_protocol::{CompressionThreshold, Encode, Packet, VarInt};

use crate::{
    net::{MAX_PACKET_LEN_SIZE, MAX_PACKET_SIZE},
    singleton::ring::McBuf,
};

pub struct PacketEncoder {
    compress_buf: Vec<u8>,
    threshold: CompressionThreshold,
}

impl std::fmt::Debug for PacketEncoder {
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
#[derive(Debug)]
#[repr(packed)]
pub struct PacketWriteInfo {
    pub start_ptr: *const u8,
    pub len: u32,
}

unsafe impl Send for PacketWriteInfo {}
unsafe impl Sync for PacketWriteInfo {}

impl PacketEncoder {
    pub fn new(threshold: CompressionThreshold) -> Self {
        Self {
            compress_buf: Vec::new(),
            threshold,
        }
    }

    pub fn compression_threshold(&self) -> CompressionThreshold {
        self.threshold
    }

    pub fn append_packet<P>(
        &mut self,
        pkt: &P,
        buf: &mut impl McBuf,
    ) -> anyhow::Result<PacketWriteInfo>
    where
        P: Packet + Encode,
    {
        let has_compression = self.threshold.0 >= 0;

        // having compression we have two [`VarInt`] headers
        let data_write_start = if has_compression {
            MAX_PACKET_LEN_SIZE * 2
        } else {
            MAX_PACKET_LEN_SIZE
        } as u64;

        // todo: does MAX_PACKET_SIZE include the VarInt header?
        let slice = buf.get_contiguous(MAX_PACKET_SIZE);

        let mut cursor = Cursor::new(slice);
        cursor.set_position(data_write_start);

        pkt.encode_with_id(&mut cursor)?;

        let data_len = cursor.position() as usize - data_write_start as usize;

        if has_compression && data_len > self.threshold.0.unsigned_abs() as usize {
            use flate2::{bufread::ZlibEncoder, Compression};

            // re-use for reading
            // todo: feel we should limit the size of the buffer
            cursor.set_position(data_write_start);
            let mut z = ZlibEncoder::new(&mut cursor, Compression::new(4));

            self.compress_buf.clear();

            let data_len_size = VarInt(data_len as i32).written_size();

            let packet_len = data_len_size + z.read_to_end(&mut self.compress_buf)?;

            ensure!(
                packet_len <= MAX_PACKET_SIZE,
                "packet exceeds maximum length"
            );

            drop(z);

            // reset cursor again
            let packet_len_size = VarInt(packet_len as i32).written_size() as u64;
            let data_len_size = VarInt(data_len as i32).written_size() as u64;
            let metadata_len = packet_len_size + data_len_size;

            let metadata_start_idx = data_write_start - metadata_len;

            cursor.set_position(metadata_start_idx);

            VarInt(packet_len as i32).encode(&mut cursor)?;
            VarInt(data_len as i32).encode(&mut cursor)?;

            let compressed_len = self.compress_buf.len();

            let end_len = data_write_start as usize + compressed_len;

            let slice = cursor.into_inner();

            slice[data_write_start as usize..end_len].copy_from_slice(&self.compress_buf);

            let entire_slice = &slice[metadata_start_idx as usize..end_len];

            let start_ptr = entire_slice.as_ptr();
            let len = entire_slice.len();

            trace!("advancing by {end_len}");
            buf.advance(end_len);

            return Ok(PacketWriteInfo {
                start_ptr: start_ptr.cast(),
                len: len as u32,
            });
        }

        let packet_len_size = VarInt(data_len as i32).written_size();

        let packet_len = packet_len_size + data_len;
        ensure!(
            packet_len <= MAX_PACKET_SIZE,
            "packet exceeds maximum length"
        );

        let metadata_start_idx = data_write_start - packet_len_size as u64;

        cursor.set_position(metadata_start_idx);

        VarInt(data_len as i32).encode(&mut cursor)?;

        let slice = cursor.into_inner();

        let end_len = data_write_start as usize + data_len;

        let entire_slice = &slice[metadata_start_idx as usize..end_len];

        let start_ptr = entire_slice.as_ptr();
        let len = entire_slice.len();

        trace!("advancing by {end_len}");
        buf.advance(end_len);

        Ok(PacketWriteInfo {
            start_ptr: start_ptr.cast(),
            len: len as u32,
        })
    }

    pub fn set_compression(&mut self, threshold: CompressionThreshold) {
        self.threshold = threshold;
    }
}
