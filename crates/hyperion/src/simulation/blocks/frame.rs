#![allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
use glam::{IVec2, IVec3};
use ndarray::ArrayView3;
use valence_generated::block::BlockState;

use crate::simulation::blocks::{Blocks, chunk::START_Y};

impl Blocks {
    #[deprecated = "this is called automatically"]
    pub fn mark_should_update(&mut self, position: IVec3) {
        let x = position.x;
        let z = position.z;

        let chunk_x = x >> 4;
        let chunk_z = z >> 4;

        let Some((index, ..)) = self.chunk_cache.get_full(&IVec2::new(chunk_x, chunk_z)) else {
            return;
        };

        self.should_update.insert(index as u32);
    }

    pub fn paste(&mut self, offset: IVec3, frame: ArrayView3<'_, BlockState>) {
        let (width, height, depth) = frame.dim();
        let start = offset;
        let end = start
            + IVec3::new(
                i32::try_from(width).unwrap() - 1,
                i32::try_from(height).unwrap() - 1,
                i32::try_from(depth).unwrap() - 1,
            );

        // Get all unique chunk positions
        let start_chunk = IVec3::new(start.x >> 4, start.y >> 4, start.z >> 4);
        let end_chunk = IVec3::new(end.x >> 4, end.y >> 4, end.z >> 4);
        for section_x in start_chunk.x..=end_chunk.x {
            for section_z in start_chunk.z..=end_chunk.z {
                let Some((idx, _, loaded_chunk)) = self
                    .chunk_cache
                    .get_full_mut(&IVec2::new(section_x, section_z))
                else {
                    continue;
                };

                self.should_update.insert(idx as u32);

                let chunk = &mut loaded_chunk.data;

                for section_y in start_chunk.y..=end_chunk.y {
                    let section_y = section_y as i16;
                    let section_idx = (section_y - (START_Y / 16)) as usize;

                    let section = &mut chunk.sections[section_idx];

                    // idx is yzx
                    // todo: section.set_delta(idx, block)
                    // Calculate the bounds for this section
                    let section_start =
                        IVec3::new(section_x * 16, i32::from(section_y * 16), section_z * 16);
                    let section_end = section_start + IVec3::new(15, 15, 15);

                    // Iterate over the intersection of the frame and this section
                    let iter_start = start.max(section_start);
                    let iter_end = end.min(section_end);

                    #[expect(clippy::excessive_nesting)]
                    for y in iter_start.y..=iter_end.y {
                        for z in iter_start.z..=iter_end.z {
                            for x in iter_start.x..=iter_end.x {
                                let frame_pos = IVec3::new(x, y, z) - start;
                                let block = frame[[
                                    frame_pos.x as usize,
                                    frame_pos.y as usize,
                                    frame_pos.z as usize,
                                ]];

                                let idx = ((y & 15) << 8) | ((z & 15) << 4) | (x & 15);
                                let idx = idx as u16;
                                section.set_delta(idx, block);
                            }
                        }
                    }
                }
            }
        }
    }
}
