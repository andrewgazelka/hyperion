use std::hint::black_box;

use bvh_region::{Bvh, TrivialHeuristic, random_aabb};
use geometry::aabb::Aabb;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tango_bench::{IntoBenchmarks, benchmark_fn, tango_benchmarks, tango_main};

const COUNT: usize = 10_000;

fn build_tree(elements: Vec<Aabb>) -> Bvh<Aabb> {
    Bvh::build::<TrivialHeuristic>(elements)
}

fn bvh_benchmarks() -> impl IntoBenchmarks {
    // Thread pool setup
    rayon::ThreadPoolBuilder::default()
        .build_global()
        .expect("Failed to build global thread pool");

    // Set seed for reproducibility
    fastrand::seed(7);

    [
        benchmark_fn("build_sparse_bvh", |b| {
            let sparse_build_elements = (0..COUNT)
                .map(|_| random_aabb(10_000.0))
                .collect::<Vec<_>>();
            b.iter(move || build_tree(sparse_build_elements.clone()))
        }),
        benchmark_fn("build_dense_bvh", |b| {
            let dense_build_elements = (0..COUNT).map(|_| random_aabb(100.0)).collect::<Vec<_>>();
            b.iter(move || build_tree(dense_build_elements.clone()))
        }),
        benchmark_fn("build_very_dense_bvh", |b| {
            let very_dense_build_elements =
                (0..COUNT).map(|_| random_aabb(1.0)).collect::<Vec<_>>();
            b.iter(move || build_tree(very_dense_build_elements.clone()))
        }),
        benchmark_fn("collisions_sparse_bvh", |b| {
            let sparse_tree = build_tree((0..COUNT).map(|_| random_aabb(10_000.0)).collect());
            let sparse_aabbs = (0..COUNT)
                .map(|_| random_aabb(10_000.0))
                .collect::<Vec<_>>();
            b.iter(move || {
                (0..COUNT).into_par_iter().for_each(|i| {
                    sparse_tree.get_collisions(sparse_aabbs[i], |elem| {
                        black_box(elem);
                        true
                    });
                });
            })
        }),
        benchmark_fn("collisions_dense_bvh", |b| {
            let dense_tree = build_tree((0..COUNT).map(|_| random_aabb(100.0)).collect());
            let dense_aabbs = (0..COUNT).map(|_| random_aabb(100.0)).collect::<Vec<_>>();
            b.iter(move || {
                (0..COUNT).into_par_iter().for_each(|i| {
                    dense_tree.get_collisions(dense_aabbs[i], |elem| {
                        black_box(elem);
                        true
                    });
                });
            })
        }),
        benchmark_fn("collisions_very_dense_bvh", |b| {
            let very_dense_tree = build_tree((0..COUNT).map(|_| random_aabb(1.0)).collect());
            let very_dense_aabbs = (0..COUNT).map(|_| random_aabb(1.0)).collect::<Vec<_>>();
            b.iter(move || {
                (0..COUNT).into_par_iter().for_each(|i| {
                    let mut collision_count = 0;
                    very_dense_tree.get_collisions(very_dense_aabbs[i], |elem| {
                        black_box(elem);
                        collision_count += 1;
                        true
                    });
                });
            })
        }),
        benchmark_fn("collisions_very_dense_bvh_limited", |b| {
            let very_dense_tree = build_tree((0..COUNT).map(|_| random_aabb(1.0)).collect());
            let very_dense_aabbs = (0..COUNT).map(|_| random_aabb(1.0)).collect::<Vec<_>>();
            b.iter(move || {
                let max_collisions = 10;
                (0..COUNT).into_par_iter().for_each(|i| {
                    let mut collision_count = 0;
                    very_dense_tree.get_collisions(very_dense_aabbs[i], |elem| {
                        black_box(elem);
                        collision_count += 1;
                        collision_count < max_collisions
                    });
                });
            })
        }),
    ]
}

tango_benchmarks!(bvh_benchmarks());

tango_main!();
