use std::io::Write;

use glam::IVec2;
use valence_protocol::{
    packets::play::{chunk_delta_update_s2c::ChunkDeltaUpdateEntry, ChunkDeltaUpdateS2c},
    ChunkSectionPos, Encode, Packet, VarInt,
};

use crate::{
    simulation::blocks::{
        chunk::{Column, START_Y},
        loader::parse::section::Section,
    },
    PacketBundle,
};

#[derive(derive_more::Debug)]
pub struct DeltaDrainPacket<'a> {
    position: ChunkSectionPos,
    #[debug(skip)]
    section: &'a mut Section,
}

impl PacketBundle for DeltaDrainPacket<'_> {
    fn encode_including_ids(self, mut write: impl Write) -> anyhow::Result<()> {
        VarInt(ChunkDeltaUpdateS2c::ID).encode(&mut write)?;

        self.position.encode(&mut write)?;

        let deltas = &mut self.section.changed_since_last_tick;
        let len = deltas.len();
        VarInt(i32::try_from(len)?).encode(&mut write)?;

        for delta_idx in deltas.iter() {
            let block_state =
                unsafe { self.section.block_states.get_unchecked(delta_idx as usize) };

            // Convert delta (u16) to y, z, x
            let y = (delta_idx >> 8) & 0xF;
            let z = (delta_idx >> 4) & 0xF;
            let x = delta_idx & 0xF;

            let entry = ChunkDeltaUpdateEntry::new()
                .with_off_x(x as u8)
                .with_off_y(y as u8)
                .with_off_z(z as u8)
                .with_block_state(u32::from(block_state));

            entry.encode(&mut write)?;
        }

        deltas.clear();

        self.section.reset_tick_deltas();

        Ok(())
    }
}

#[derive(derive_more::Debug)]
pub struct DeltaPacket<'a> {
    position: ChunkSectionPos,
    #[debug(skip)]
    section: &'a Section,
}

impl PacketBundle for DeltaPacket<'_> {
    fn encode_including_ids(self, mut write: impl Write) -> anyhow::Result<()> {
        VarInt(ChunkDeltaUpdateS2c::ID).encode(&mut write)?;

        self.position.encode(&mut write)?;

        let deltas = &self.section.changed;
        let len = deltas.len();
        VarInt(i32::try_from(len)?).encode(&mut write)?;

        for delta_idx in deltas {
            let block_state =
                unsafe { self.section.block_states.get_unchecked(delta_idx as usize) };

            // Convert delta (u16) to y, z, x
            let y = (delta_idx >> 8) & 0xF;
            let z = (delta_idx >> 4) & 0xF;
            let x = delta_idx & 0xF;

            let entry = ChunkDeltaUpdateEntry::new()
                .with_off_x(x as u8)
                .with_off_y(y as u8)
                .with_off_z(z as u8)
                .with_block_state(u32::from(block_state));

            entry.encode(&mut write)?;
        }

        Ok(())
    }
}

impl Column {
    pub fn delta_drain_packets(&mut self) -> impl Iterator<Item = DeltaDrainPacket<'_>> + '_ {
        let IVec2 { x, y: z } = self.position;

        self.data
            .sections
            .iter_mut()
            .enumerate()
            .filter(|(_, section)| !section.changed_since_last_tick.is_empty())
            .map(move |(i, section)| {
                let y = i32::try_from(i).unwrap();
                let y = y + i32::from(START_Y >> 4);

                DeltaDrainPacket {
                    position: ChunkSectionPos::new(x, y, z),
                    section,
                }
            })
    }

    pub fn original_delta_packets(&self) -> impl Iterator<Item = DeltaPacket<'_>> + '_ {
        let IVec2 { x, y: z } = self.position;

        self.data
            .sections
            .iter()
            .enumerate()
            .filter(|(_, section)| !section.changed.is_empty())
            .map(move |(i, section)| {
                let y = i32::try_from(i).unwrap();
                let y = y + i32::from(START_Y >> 4);

                DeltaPacket {
                    position: ChunkSectionPos::new(x, y, z),
                    section,
                }
            })
    }
}
