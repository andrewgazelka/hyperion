use std::hint::black_box;

use bvh_region::{aabb::Aabb, random_aabb, Bvh, TrivialHeuristic};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tango_bench::{
    benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks, MeasurementSettings,
};

const COUNT: usize = 10_000;

fn benchmark_build_4_cores(elements: Vec<Aabb>) {
    build_tree(elements);
}

fn build_tree(elements: Vec<Aabb>) -> Bvh<Aabb> {
    Bvh::build::<TrivialHeuristic>(elements)
}

fn build_benchmarks() -> impl IntoBenchmarks {
    // thread pool
    rayon::ThreadPoolBuilder::default()
        .build_global()
        .expect("Failed to build global thread pool");

    // so reproducible
    fastrand::seed(7);

    let sparse_build_elements = (0..COUNT)
        .map(|_| random_aabb(10_000.0))
        .collect::<Vec<_>>();

    let dense_build_elements = (0..COUNT).map(|_| random_aabb(100.0)).collect::<Vec<_>>();

    let very_dense_build_elements: Vec<_> = (0..COUNT).map(|_| random_aabb(1.0)).collect();

    let sparse_tree = build_tree(sparse_build_elements.clone());
    let dense_tree = build_tree(dense_build_elements.clone());
    let very_dense_tree = build_tree(very_dense_build_elements.clone());
    let very_dense_tree2 = very_dense_tree.clone();

    let sparse_aabbs = (0..COUNT)
        .map(|_| random_aabb(10_000.0))
        .collect::<Vec<_>>();

    let dense_aabbs = (0..COUNT).map(|_| random_aabb(100.0)).collect::<Vec<_>>();

    let very_dense_aabbs = (0..COUNT).map(|_| random_aabb(1.0)).collect::<Vec<_>>();
    let very_dense_aabbs2 = very_dense_aabbs.clone();

    [
        benchmark_fn("build_sparse_bvh", move || {
            benchmark_build_4_cores(sparse_build_elements.clone());
        }),
        benchmark_fn("build_dense_bvh", move || {
            benchmark_build_4_cores(dense_build_elements.clone());
        }),
        benchmark_fn("build_very_dense_bvh", move || {
            benchmark_build_4_cores(very_dense_build_elements.clone());
        }),
        benchmark_fn("collisions_sparse_bvh", move || {
            (0..COUNT).into_par_iter().for_each(|i| {
                sparse_tree.get_collisions(sparse_aabbs[i], |elem| {
                    black_box(elem);
                    true
                });
            });
        }),
        benchmark_fn("collisions_dense_bvh", move || {
            (0..COUNT).into_par_iter().for_each(|i| {
                dense_tree.get_collisions(dense_aabbs[i], |elem| {
                    black_box(elem);
                    true
                });
            });
        }),
        benchmark_fn("collisions_very_dense_bvh", move || {
            (0..COUNT).into_par_iter().for_each(|i| {
                let mut collision_count = 0;

                very_dense_tree.get_collisions(very_dense_aabbs[i], |elem| {
                    black_box(elem);
                    collision_count += 1;

                    true
                });
            });
        }),
        benchmark_fn("collisions_very_dense_bvh_limited", move || {
            let max_collisions = 10;
            (0..COUNT).into_par_iter().for_each(|i| {
                let mut collision_count = 0;

                very_dense_tree2.get_collisions(very_dense_aabbs2[i], |elem| {
                    black_box(elem);
                    collision_count += 1;

                    collision_count < max_collisions
                });
            });
        }),
    ]
}

tango_benchmarks!(build_benchmarks());

tango_main!(MeasurementSettings {
    min_iterations_per_sample: 10,
    max_iterations_per_sample: 10_000,
    yield_before_sample: true,
    ..Default::default()
});
