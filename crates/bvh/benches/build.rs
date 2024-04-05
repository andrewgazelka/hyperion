use std::hint::black_box;

use bvh::{aabb::Aabb, create_random_elements_1, random_aabb, Bvh, TrivialHeuristic};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};

const COUNT: usize = 10_000;

fn benchmark_1m_4_cores(elements: Vec<Aabb>) {
    let threads = 4;
    let thread_pool = rayon::ThreadPoolBuilder::default()
        .num_threads(threads)
        .build()
        .expect("Failed to build global thread pool");

    thread_pool.install(|| {
        build_tree(elements);
    });
}

fn build_tree(elements: Vec<Aabb>) -> Bvh<Aabb> {
    Bvh::build::<TrivialHeuristic>(elements)
}

fn build_benchmarks() -> impl IntoBenchmarks {
    // so reproducible
    fastrand::seed(7);

    let elements = (0..COUNT)
        .map(|_| random_aabb(10_000.0))
        .collect::<Vec<_>>();

    let tree = build_tree(elements);

    let aabs = (0..COUNT)
        .map(|_| random_aabb(10_000.0))
        .collect::<Vec<_>>();

    let elements_to_build_tree = create_random_elements_1(COUNT, 10_000.0);

    [
        benchmark_fn("build", move || {
            benchmark_1m_4_cores(elements_to_build_tree.clone())
        }),
        benchmark_fn("collisions", move || {
            (0..COUNT).into_par_iter().for_each(|i| {
                tree.get_collisions(aabs[i], |elem| {
                    black_box(elem);
                });
            });
        }),
    ]
}

tango_benchmarks!(build_benchmarks());
tango_main!();
