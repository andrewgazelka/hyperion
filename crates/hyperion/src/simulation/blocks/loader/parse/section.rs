use std::collections::{hash_map::Entry, HashMap};

use fxhash::FxBuildHasher;
use indexmap::IndexSet;
use more_asserts::{debug_assert_le, debug_assert_lt};
use roaring::RoaringBitmap;
use valence_generated::block::BlockState;
use valence_server::layer::chunk::{BiomeContainer, BlockStateContainer};

#[derive(Clone, Debug)]
pub struct Section {
    pub block_states: BlockStateContainer,
    pub biomes: BiomeContainer,

    // todo: maybe make stack array of 2048
    pub block_light: [u8; 2048],
    pub sky_light: [u8; 2048],

    // goes up to 2^15 in 1.20.1 vanilla mc we can use u16
    // pub original: HashMap<u16, BlockState, FxBuildHasher>,

    // index of the block that has changed
    pub deltas_since_prev_tick: RoaringBitmap,
}

impl Section {
    pub fn set(&mut self, idx: u16, new: BlockState) -> BlockState {
        self.block_states.set(idx as usize, new)
    }

    // returns true if the block state was changed
    pub fn set_delta(&mut self, idx: u16, new: BlockState) -> BlockState {
        debug_assert_lt!(idx, 4096);

        let before = self.block_states.set(idx as usize, new);

        if before != new {
            self.block_states.set(idx as usize, new);
            self.deltas_since_prev_tick.insert(idx as u32);
        }

        new
    }

    pub fn reset_tick_deltas(&mut self) {
        self.deltas_since_prev_tick.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_section() -> Section {
        Section {
            block_states: BlockStateContainer::new(),
            biomes: BiomeContainer::new(),
            block_light: [0; 2048],
            sky_light: [0; 2048],
            original: HashMap::with_hasher(FxBuildHasher::default()),
            deltas_since_prev_tick: FxHashSet::default(),
        }
    }

    #[test]
    fn test_section_set_new_block() {
        let mut section = create_test_section();
        let new_state = BlockState::STONE;

        let result = section.set(0, new_state);
        assert_eq!(result, BlockState::AIR);
        assert_eq!(section.block_states.get(0), new_state);
        assert_eq!(section.original.len(), 1);
        assert!(section.deltas_since_prev_tick.contains(&0));
    }

    #[test]
    fn test_section_set_same_block() {
        let mut section = create_test_section();
        let state = BlockState::STONE;

        section.set(0, state);
        let result = section.set(0, state);
        assert_eq!(result, state);
        assert_eq!(section.original.len(), 1);
    }

    #[test]
    fn test_section_set_revert_block() {
        let mut section = create_test_section();
        let new_state = BlockState::STONE;

        section.set(0, new_state);
        let result = section.set(0, BlockState::AIR);
        assert_eq!(result, new_state);
        assert!(section.original.is_empty());
        assert!(section.deltas_since_prev_tick.contains(&0));
    }

    #[test]
    fn test_section_set_multiple_blocks() {
        let mut section = create_test_section();
        let states = [BlockState::STONE, BlockState::DIRT, BlockState::GRASS_BLOCK];

        for (i, &state) in states.iter().enumerate() {
            section.set(i as u16, state);
        }

        assert_eq!(section.original.len(), 3);
        assert_eq!(section.deltas_since_prev_tick.len(), 3);

        for (i, &state) in states.iter().enumerate() {
            assert_eq!(section.block_states.get(i), state);
        }
    }

    #[test]
    fn test_section_set_boundary_values() {
        let mut section = create_test_section();
        let state = BlockState::STONE;

        // Test setting the first block
        section.set(0, state);
        assert_eq!(section.block_states.get(0), state);

        // Test setting the last block (assuming 4096 blocks per section)
        section.set(4095, state);
        assert_eq!(section.block_states.get(4095), state);
    }

    #[test]
    fn test_reset_tick_deltas() {
        let mut section = create_test_section();

        section.set(0, BlockState::STONE);
        section.set(1, BlockState::DIRT);
        assert_eq!(section.deltas_since_prev_tick.len(), 2);

        section.reset_tick_deltas();
        assert!(section.deltas_since_prev_tick.is_empty());
        assert_eq!(section.original.len(), 2);
    }

    #[test]
    fn test_section_set_multiple_changes() {
        let mut section = create_test_section();

        section.set(0, BlockState::STONE);
        section.set(0, BlockState::DIRT);
        section.set(0, BlockState::GRASS_BLOCK);

        assert_eq!(section.original.len(), 1);
        assert_eq!(section.block_states.get(0), BlockState::GRASS_BLOCK);
    }
}
