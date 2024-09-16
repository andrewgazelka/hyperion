use derive_more::From;
use glam::{I16Vec2, IVec3};
use valence_generated::block::BlockState;

use crate::simulation::blocks::{chunk::START_Y, MinecraftWorld};

#[derive(From)]
pub struct Frame {
    elems: ndarray::Array3<BlockState>,
}

impl Frame {
    pub fn paste(&self, offset: IVec3, mc: &mut MinecraftWorld) {
        let (width, height, depth) = self.elems.dim();
        let start = offset;
        let end = start + IVec3::new(width as i32 - 1, height as i32 - 1, depth as i32 - 1);

        // Get all unique chunk positions
        let start_chunk = IVec3::new(start.x >> 4, start.y >> 4, start.z >> 4).as_i16vec3();
        let end_chunk = IVec3::new(end.x >> 4, end.y >> 4, end.z >> 4).as_i16vec3();
        for section_x in start_chunk.x..=end_chunk.x {
            for section_z in start_chunk.z..=end_chunk.z {
                let Some(loaded_chunk) =
                    mc.get_loaded_chunk_mut(I16Vec2::new(section_x, section_z))
                else {
                    continue;
                };

                let chunk = &mut loaded_chunk.chunk;

                for section_y in start_chunk.y..=end_chunk.y {
                    let section_idx = (section_y - (START_Y / 16) as i16) as usize;

                    let section = &mut chunk.sections[section_idx];

                    // idx is yzx
                    // todo: section.set_delta(idx, block)
                    // Calculate the bounds for this section
                    let section_start = IVec3::new(
                        section_x as i32 * 16,
                        section_y as i32 * 16,
                        section_z as i32 * 16,
                    );
                    let section_end = section_start + IVec3::new(15, 15, 15);

                    // Iterate over the intersection of the frame and this section
                    let iter_start = start.max(section_start);
                    let iter_end = end.min(section_end);

                    for y in iter_start.y..=iter_end.y {
                        for z in iter_start.z..=iter_end.z {
                            for x in iter_start.x..=iter_end.x {
                                let frame_pos = IVec3::new(x, y, z) - start;
                                let block = self.elems[[
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
