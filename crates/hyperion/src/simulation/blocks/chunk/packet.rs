use std::io::Write;

use glam::I16Vec2;
use valence_protocol::{
    packets::play::{chunk_delta_update_s2c::ChunkDeltaUpdateEntry, ChunkDeltaUpdateS2c},
    ChunkSectionPos, Encode, Packet, VarInt,
};

use crate::{
    simulation::blocks::{
        chunk::{LoadedChunk, START_Y},
        loader::parse::section::Section,
    },
    PacketBundle,
};

#[derive(derive_more::Debug)]
pub struct DeltaPacket<'a> {
    position: ChunkSectionPos,
    #[debug(skip)]
    section: &'a mut Section,
}

impl PacketBundle for DeltaPacket<'_> {
    fn encode_including_ids(self, mut write: impl Write) -> anyhow::Result<()> {
        VarInt(ChunkDeltaUpdateS2c::ID).encode(&mut write)?;

        self.position.encode(&mut write)?;

        let deltas = &mut self.section.deltas_since_prev_tick;
        let len = deltas.len();
        VarInt(len as i32).encode(&mut write)?;

        for delta_idx in deltas.iter() {
            let block_state = self.section.block_states.get(delta_idx as usize);

            // Convert delta (u16) to y, z, x
            let y = (delta_idx >> 8) & 0xF;
            let z = (delta_idx >> 4) & 0xF;
            let x = delta_idx & 0xF;

            let entry = ChunkDeltaUpdateEntry::new()
                .with_off_x(x as u8)
                .with_off_y(y as u8)
                .with_off_z(z as u8)
                .with_block_state(block_state.to_raw() as u32);

            entry.encode(&mut write)?;
        }

        deltas.clear();

        self.section.reset_tick_deltas();

        Ok(())
    }
}

impl LoadedChunk {
    pub fn delta_packets(&mut self) -> impl Iterator<Item = DeltaPacket<'_>> + '_ {
        let I16Vec2 { x, y: z } = self.position;
        let x = i32::from(x);
        let z = i32::from(z);

        self.chunk
            .sections
            .iter_mut()
            .enumerate()
            .filter(|(_, section)| !section.deltas_since_prev_tick.is_empty())
            .map(move |(i, section)| {
                let y = i as i32;
                let y = y + (START_Y >> 4);

                DeltaPacket {
                    position: ChunkSectionPos::new(x, y, z),
                    section,
                }
            })
    }
}