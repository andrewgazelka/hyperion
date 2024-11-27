#![feature(portable_simd)]
#![feature(trusted_len)]

use core::simd;
use std::{
    iter::zip,
    simd::{cmp::SimdPartialEq, LaneCount, Mask, MaskElement, Simd, SupportedLaneCount},
};

use crate::one_bit_positions::OneBitPositionsExt;

mod one_bit_positions;

pub fn copy_and_get_diff<T, const LANES: usize>(
    prev: &mut [T],
    current: &[T],
    mut on_diff: impl FnMut(usize, &T, &T),
) where
    Simd<T, LANES>: AsMut<[T; LANES]> + SimdPartialEq,
    T: simd::SimdElement + PartialEq + std::fmt::Debug,
    <T as simd::SimdElement>::Mask: MaskElement,
    LaneCount<LANES>: SupportedLaneCount,
    <Simd<T, LANES> as SimdPartialEq>::Mask: Into<Mask<<T as simd::SimdElement>::Mask, LANES>>,
{
    debug_assert_eq!(
        prev.len(),
        current.len(),
        "prev and current must have the same length"
    );

    let (before_prev, prev, after_prev) = prev.as_simd_mut::<LANES>();
    let (before_current, current, after_current) = current.as_simd::<LANES>();

    debug_assert_eq!(
        before_prev.len(),
        before_current.len(),
        "before_prev and before_current must have the same length"
    );

    debug_assert_eq!(
        prev.len(),
        current.len(),
        "prev and current must have the same length"
    );

    debug_assert_eq!(
        after_prev.len(),
        after_current.len(),
        "after_prev and after_current must have the same length"
    );

    let mut idx = 0;

    for (prev, current) in zip(before_prev, before_current) {
        if prev != current {
            debug_assert_ne!(prev, current);
            on_diff(idx, prev, current);
        }

        *prev = *current;
        idx += 1;
    }

    for (prev, current) in zip(prev, current) {
        let not_equal = prev.simd_ne(*current);
        let not_equal = not_equal.into();

        let bitmask = Mask::to_bitmask(not_equal);

        for local_idx in bitmask.one_positions() {
            let prev = prev[local_idx];
            let current = current[local_idx];

            debug_assert_ne!(prev, current);

            on_diff(idx + local_idx, &prev, &current);
        }

        idx += LANES;

        *prev = *current;
    }

    for (prev, current) in zip(after_prev, after_current) {
        if prev != current {
            debug_assert_ne!(prev, current);
            on_diff(idx, prev, current);
        }

        *prev = *current;
        idx += 1;
    }
}

#[cfg(test)]
mod tests {
    const LANES: usize = 8;

    use std::fmt::Debug;

    use aligned_vec::{AVec, RuntimeAlign};
    use proptest::prelude::*;

    use super::*;

    // Helper function to collect differences
    fn collect_diffs<T>(prev_raw: &[T], current_raw: &[T]) -> Vec<(usize, T, T)>
    where
        Simd<T, LANES>: AsMut<[T; LANES]> + SimdPartialEq,
        T: simd::SimdElement + PartialEq + Debug,
        <T as simd::SimdElement>::Mask: MaskElement,
        LaneCount<LANES>: SupportedLaneCount,
        <Simd<T, LANES> as SimdPartialEq>::Mask: Into<Mask<<T as simd::SimdElement>::Mask, LANES>>,
    {
        // convert prev and current to simd-aligned arrays
        let mut prev: AVec<T, RuntimeAlign> = AVec::from_iter(64, prev_raw.iter().copied());
        let current: AVec<T, RuntimeAlign> = AVec::from_iter(64, current_raw.iter().copied());

        let mut diffs = Vec::new();
        copy_and_get_diff::<_, LANES>(&mut prev, &current, |idx, prev, curr| {
            diffs.push((idx, *prev, *curr));
        });
        diffs
    }

    // Generate arrays of various sizes to test SIMD boundary conditions
    fn generate_array_strategy<T>(min_size: usize) -> impl Strategy<Value = Vec<T>>
    where
        T: simd::SimdElement + Arbitrary + 'static,
    {
        prop::collection::vec(any::<T>(), min_size..=min_size + LANES * 2)
    }

    // Helper to verify that all differences are captured correctly
    fn verify_differences<T>(prev: &[T], current: &[T], diffs: &[(usize, T, T)])
    where
        T: simd::SimdElement + PartialEq + Debug + Clone,
    {
        let mut expected_diffs = Vec::new();
        for (idx, (p, c)) in zip(prev, current).enumerate() {
            if p != c {
                expected_diffs.push((idx, *p, *c));
            }
        }
        assert_eq!(
            diffs,
            expected_diffs.as_slice(),
            "Differences don't match expected for prev={prev:?} and current={current:?}"
        );
    }

    proptest! {
        // Test with u32 arrays of various sizes
        #[test]
        fn test_u32_arrays(
            current in generate_array_strategy::<u32>(LANES * 2)
        ) {
            let mut prev = current.clone();
            // Modify some elements to create differences
            if !prev.is_empty() {
                let prev_len = prev.len();
                prev[prev_len / 2] = prev[prev_len / 2].wrapping_add(1);
                if prev_len > 1 {
                    prev[0] = prev[0].wrapping_add(1);
                }
            }

            let diffs = collect_diffs(&prev, &current);
            verify_differences(&prev, &current, &diffs);
        }

        // Test with i32 arrays including negative numbers
        #[test]
        fn test_i32_arrays(
            current in generate_array_strategy::<i32>(LANES * 2)
        ) {
            let mut prev = current.clone();
            if !prev.is_empty() {
                let prev_len = prev.len();
                prev[prev_len / 2] = prev[prev_len / 2].wrapping_add(1);
                if prev_len > 1 {
                    prev[0] = prev[0].wrapping_sub(1);
                }
            }

            let diffs = collect_diffs(&prev, &current);
            verify_differences(&prev, &current, &diffs);
        }

        // Test with exact SIMD lane size
        #[test]
        fn test_exact_lane_size(
            current in generate_array_strategy::<u32>(LANES)
        ) {
            let mut prev = current.clone();
            if !prev.is_empty() {
                prev[0] = prev[0].wrapping_add(1);
            }

            let diffs = collect_diffs(&prev, &current);
            verify_differences(&prev, &current, &diffs);
        }

        // Test with arrays smaller than SIMD lane size
        #[test]
        fn test_small_arrays(
            current in generate_array_strategy::<u32>(LANES / 2)
        ) {
            let mut prev = current.clone();
            if !prev.is_empty() {
                prev[0] = prev[0].wrapping_add(1);
            }

            let diffs = collect_diffs(&prev, &current);
            verify_differences(&prev, &current, &diffs);
        }

        // Test with no differences
        #[test]
        fn test_no_differences(
            current in generate_array_strategy::<u32>(LANES * 2)
        ) {
            let diffs = collect_diffs(&current, &current);
            assert!(diffs.is_empty(), "Expected no differences");
        }

        // Test with all elements different
        #[test]
        fn test_all_different(
            current in generate_array_strategy::<u32>(LANES * 2)
        ) {
            let prev = current.iter()
                .map(|x| x.wrapping_add(1))
                .collect::<Vec<_>>();

            let diffs = collect_diffs(&prev, &current);
            verify_differences(&prev, &current, &diffs);
        }

        // Test edge case with alternating differences
        #[test]
        fn test_alternating_differences(
            current in generate_array_strategy::<u32>(LANES * 2)
        ) {
            let mut prev = current.clone();
            for i in (0..prev.len()).step_by(2) {
                prev[i] = prev[i].wrapping_add(1);
            }

            let diffs = collect_diffs(&prev, &current);
            verify_differences(&prev, &current, &diffs);
        }
    }
}
