use std::io::Read;

use anyhow::ensure;
use valence_protocol::{CompressionThreshold, Encode, Packet, VarInt};

use crate::net::{MaybeRegisteredBuffer, MAX_PACKET_SIZE};

pub struct PacketEncoder {
    pub buf: MaybeRegisteredBuffer,
    compress_buf: Vec<u8>,
    threshold: CompressionThreshold,
}

impl PacketEncoder {
    pub fn new(threshold: CompressionThreshold) -> Self {
        Self {
            buf: MaybeRegisteredBuffer::default(),
            compress_buf: Vec::new(),
            threshold,
        }
    }

    pub fn append_packet<P>(&mut self, pkt: &P) -> anyhow::Result<()>
    where
        P: Packet + Encode,
    {
        let start_len = self.buf.len();

        pkt.encode_with_id(&mut self.buf)?;

        let data_len = self.buf.len() - start_len;

        if self.threshold.0 >= 0 {
            use flate2::{bufread::ZlibEncoder, Compression};

            if data_len > self.threshold.0 as usize {
                let mut z = ZlibEncoder::new(&self.buf[start_len..], Compression::new(4));

                self.compress_buf.clear();

                let data_len_size = VarInt(data_len as i32).written_size();

                let packet_len = data_len_size + z.read_to_end(&mut self.compress_buf)?;

                ensure!(
                    packet_len <= MAX_PACKET_SIZE as usize,
                    "packet exceeds maximum length"
                );

                drop(z);

                self.buf.truncate(start_len);

                VarInt(packet_len as i32).encode(&mut self.buf)?;
                VarInt(data_len as i32).encode(&mut self.buf)?;
                self.buf.extend_from_slice(&self.compress_buf);
            } else {
                let data_len_size = 1;
                let packet_len = data_len_size + data_len;

                ensure!(
                    packet_len <= MAX_PACKET_SIZE as usize,
                    "packet exceeds maximum length"
                );

                let packet_len_size = VarInt(packet_len as i32).written_size();

                let data_prefix_len = packet_len_size + data_len_size;

                self.buf.put_bytes(0, data_prefix_len);
                self.buf
                    .copy_within(start_len..start_len + data_len, start_len + data_prefix_len);

                let mut front = &mut self.buf[start_len..];

                VarInt(packet_len as i32).encode(&mut front)?;
                // Zero for no compression on this packet.
                VarInt(0).encode(front)?;
            }

            return Ok(());
        }

        let packet_len = data_len;

        ensure!(
            packet_len <= MAX_PACKET_SIZE,
            "packet exceeds maximum length"
        );

        let packet_len_size = VarInt(packet_len as i32).written_size();

        self.buf.put_bytes(0, packet_len_size);
        self.buf
            .copy_within(start_len..start_len + data_len, start_len + packet_len_size);

        let front = &mut self.buf[start_len..];
        VarInt(packet_len as i32).encode(front)?;

        Ok(())
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }

    pub fn set_compression(&mut self, threshold: CompressionThreshold) {
        self.threshold = threshold;
    }
}
