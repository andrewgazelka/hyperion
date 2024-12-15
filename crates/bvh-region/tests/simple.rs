use std::collections::HashSet;

use approx::assert_relative_eq;
use bvh_region::Bvh;
use geometry::aabb::{Aabb, OrderedAabb};
use glam::Vec3;
use proptest::prelude::*;

const fn copied<T: Copy>(value: &T) -> T {
    *value
}

// Helper function to compute the squared distance from a point to an Aabb.
// This logic might differ depending on how you define Aabb.
fn aabb_distance_squared(aabb: &Aabb, p: Vec3) -> f64 {
    let p = p.as_dvec3();
    let min = aabb.min.as_dvec3(); // Ensure you have aabb.min / aabb.max as Vec3
    let max = aabb.max.as_dvec3();
    let clamped = p.clamp(min, max);
    p.distance_squared(clamped)
}

#[test]
fn simple() {
    let elements = vec![Aabb {
        min: Vec3::new(-1.470_215_5e30, 0.0, 0.0),
        max: Vec3::new(0.0, 0.0, 0.0),
    }];

    let target = Vec3::new(0.0, 0.0, 0.0);
    let bvh = Bvh::build(elements.clone(), copied);
    let (closest, dist2) = bvh.get_closest(target, copied).unwrap();

    assert_eq!(closest, &elements[0]);
    assert_relative_eq!(dist2, 0.0);
}

proptest! {
    #[test]
    fn test_get_closest_correctness(
        elements in prop::collection::vec(
            // Generate random AABBs by picking two random points and making one the min and the other the max.
            (any::<f32>(), any::<f32>(), any::<f32>(), any::<f32>(), any::<f32>(), any::<f32>())
                .prop_map(|(x1, y1, z1, x2, y2, z2)| {
                    let min_x = x1.min(x2);
                    let max_x = x1.max(x2);
                    let min_y = y1.min(y2);
                    let max_y = y1.max(y2);
                    let min_z = z1.min(z2);
                    let max_z = z1.max(z2);
                    Aabb::from([min_x, min_y, min_z, max_x, max_y, max_z])
                }),
            1..50 // vary the number of elements from small to moderate
        ),
        target in (any::<f32>(), any::<f32>(), any::<f32>())
    ) {
        let target_vec = Vec3::new(target.0, target.1, target.2);
        let bvh = Bvh::build(elements.clone(), copied);

        let closest_bvh = bvh.get_closest(target_vec, copied);

        // Compute the closest element by brute force
        let mut best: Option<(&Aabb, f64)> = None;
        for aabb in &elements {
            let dist = aabb_distance_squared(aabb, target_vec);
            if let Some((_, best_dist)) = best {
                if dist < best_dist {
                    best = Some((aabb, dist));
                }
            } else {
                best = Some((aabb, dist));
            }
        }

        // Compare results
        match (closest_bvh, best) {
            (Some((bvh_aabb, bvh_dist)), Some((brute_aabb, brute_dist))) => {
                if bvh_dist.is_infinite() && brute_dist.is_infinite() {
                    // If both are infinite, they should return the same element
                    prop_assert_eq!(bvh_aabb, brute_aabb);
                } else {
                    // Check that the distances are essentially the same
                    prop_assert!((bvh_dist - brute_dist).abs() < 1e-6, "Distances differ significantly; BVH: {bvh_dist}, brute force: {brute_dist}");

                    let target = Vec3::new(target.0, target.1, target.2);
                    let calculated_bvh_dist = bvh_aabb.dist2(target);
                    let calculated_brute_dist = brute_aabb.dist2(target);
                    prop_assert!((calculated_bvh_dist - calculated_brute_dist).abs() < 1e-6, "Distances differ significantly; BVH: {calculated_bvh_dist}, brute force: {calculated_brute_dist}");

                    // We are commenting this out because there might be some cases
                    // where there are multiple "correct" answers.
                    // prop_assert_eq!(bvh_aabb, brute_aabb);
                }

            },
            (None, None) => {
                // If there are no elements, both should return None
                prop_assert!(true);
            },
            (x,y) => {
                // If one returns None and the other doesn't, there's a mismatch
                prop_assert!(false, "Mismatch between BVH closest and brute force closest; BVH: {x:?}, brute force: {y:?}");
            }
        }
    }
}

proptest! {
    #[test]
    fn test_range_correctness(
        elements in prop::collection::vec(
            (any::<f32>(), any::<f32>(), any::<f32>(), any::<f32>(), any::<f32>(), any::<f32>())
                .prop_map(|(x1, y1, z1, x2, y2, z2)| {
                    let min_x = x1.min(x2);
                    let max_x = x1.max(x2);
                    let min_y = y1.min(y2);
                    let max_y = y1.max(y2);
                    let min_z = z1.min(z2);
                    let max_z = z1.max(z2);
                    Aabb::from([min_x, min_y, min_z, max_x, max_y, max_z])
                }),
            1..50
        ),
        target in (any::<f32>(), any::<f32>(), any::<f32>(), any::<f32>(), any::<f32>(), any::<f32>())
            .prop_map(|(x1, y1, z1, x2, y2, z2)| {
                let min_x = x1.min(x2);
                let max_x = x1.max(x2);
                let min_y = y1.min(y2);
                let max_y = y1.max(y2);
                let min_z = z1.min(z2);
                let max_z = z1.max(z2);
                Aabb::from([min_x, min_y, min_z, max_x, max_y, max_z])
            })
    ) {
        let bvh = Bvh::build(elements.clone(), copied);

        // Compute brute force collisions
        let mut brute_force_set = HashSet::new();
        for aabb in &elements {
            if aabb.collides(&target) {
                let aabb = OrderedAabb::try_from(*aabb).unwrap();
                brute_force_set.insert(aabb);
            }
        }

        // Compute BVH collisions
        let mut bvh_set = HashSet::new();

        for candidate in bvh.range(target, copied) {
            // Find index of candidate in `elements`:
            let candidate = OrderedAabb::try_from(*candidate).unwrap();
            bvh_set.insert(candidate);
        }

        // Compare sets
        prop_assert_eq!(&bvh_set, &brute_force_set, "Mismatch between BVH range and brute force collision sets: {:?} != {:?}", bvh_set, brute_force_set);
    }
}
