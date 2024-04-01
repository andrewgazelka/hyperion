// https://stackoverflow.com/a/61681112/4889030
// https://matklad.github.io/2020/10/03/fast-thread-locals-in-rust.html
use std::cell::Cell;

use anyhow::ensure;
use bytes::BufMut;
use uuid::Uuid;
use valence_protocol::{math::DVec2, Encode, Packet, VarInt};

const PACKET_LEN_BYTES_MAX: usize = 3;

#[derive(Copy, Clone)]
pub enum PacketNecessity {
    #[allow(dead_code)]
    Needed,
    Droppable {
        prioritize_location: DVec2,
    },
}

#[derive(Copy, Clone)]
pub struct PacketMetadata {
    pub necessity: PacketNecessity,
    pub exclude_player: Option<Uuid>,
}

/// Packet which should not be dropped
pub struct NecessaryPacket {
    pub exclude_player: Option<Uuid>,
    pub offset: usize,
    pub len: usize,
}

/// Packet which may be dropped
pub struct DroppablePacket {
    pub prioritize_location: DVec2,
    pub exclude_player: Option<Uuid>,
    pub offset: usize,
    pub len: usize,
}

#[derive(Default)]
pub struct PacketBuffer {
    pub packet_data: Vec<u8>,
    pub necessary_packets: Vec<NecessaryPacket>,
    pub droppable_packets: Vec<DroppablePacket>,
}

impl PacketBuffer {
    pub const fn new() -> Self {
        Self {
            packet_data: Vec::new(),
            necessary_packets: Vec::new(),
            droppable_packets: Vec::new(),
        }
    }

    pub fn append_packet<P>(&mut self, pkt: &P, metadata: PacketMetadata) -> anyhow::Result<()>
    where
        P: Packet + Encode,
    {
        // Reserve MAX_PACKET_SIZE_LEN bytes before the packet to have room to write the packet
        // size without shifting the packet body. This means that there is some amount of unused
        // memory, but the amount of unused memory should be negligible.
        let mut packet_start = self.packet_data.len();
        self.packet_data
            .resize(packet_start + PACKET_LEN_BYTES_MAX, 0);

        // Write the packet data after the reserved packet length
        pkt.encode_with_id((&mut self.packet_data).writer())?;

        // Packet length excluding length of size
        let packet_len = self.packet_data.len() - packet_start - PACKET_LEN_BYTES_MAX;

        ensure!(
            packet_len <= valence_protocol::MAX_PACKET_SIZE as usize,
            "packet exceeds maximum length"
        );

        // Shift the start of the packet to the start of the packet length and write the packet
        // length there
        #[allow(clippy::cast_possible_wrap)]
        let packet_len_var_int = VarInt(packet_len as i32);
        packet_start += PACKET_LEN_BYTES_MAX - packet_len_var_int.written_size();

        #[allow(clippy::indexing_slicing)]
        let front = &mut self.packet_data[packet_start..];
        packet_len_var_int.encode(front)?;

        let packet_len_including_size = packet_len + packet_len_var_int.written_size();

        match metadata.necessity {
            PacketNecessity::Needed => {
                self.necessary_packets.push(NecessaryPacket {
                    exclude_player: metadata.exclude_player,
                    offset: packet_start,
                    len: packet_len_including_size,
                });
            }
            PacketNecessity::Droppable {
                prioritize_location,
            } => {
                self.droppable_packets.push(DroppablePacket {
                    prioritize_location,
                    exclude_player: metadata.exclude_player,
                    offset: packet_start,
                    len: packet_len_including_size,
                });
            }
        }

        Ok(())
    }

    pub fn clear_packets(&mut self) {
        self.packet_data.clear();
        self.necessary_packets.clear();
        self.droppable_packets.clear();
    }
}

#[thread_local]
static ENCODER: Cell<PacketBuffer> = Cell::new(PacketBuffer::new());

pub struct Encoder;

impl Encoder {
    pub fn append<P: Packet + Encode>(packet: &P, metadata: PacketMetadata) -> anyhow::Result<()> {
        let mut encoder = ENCODER.take();
        let result = encoder.append_packet(packet, metadata);
        ENCODER.set(encoder);
        result
    }

    pub fn par_drain<F>(f: F)
    where
        F: Fn(&mut PacketBuffer) + Sync,
    {
        rayon::broadcast(|_| {
            let mut encoder = ENCODER.take();
            f(&mut encoder);
            ENCODER.set(encoder);
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
