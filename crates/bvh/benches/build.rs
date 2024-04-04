use std::hint::black_box;

use bvh::{aabb::Aabb, create_random_elements_1, random_aabb, Bvh, TrivialHeuristic};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};

const COUNT: usize = 10_000;

fn benchmark_1m_4_cores() {
    let threads = 4;
    let thread_pool = rayon::ThreadPoolBuilder::default()
        .num_threads(threads)
        .build()
        .expect("Failed to build global thread pool");

    thread_pool.install(|| {
        build_tree();
    });
}

fn build_tree() -> Bvh<Aabb> {
    let elements = create_random_elements_1(COUNT, 10_000.0);
    Bvh::build::<TrivialHeuristic>(elements)
}

fn build_benchmarks() -> impl IntoBenchmarks {
    let tree = build_tree();

    [
        benchmark_fn("build", benchmark_1m_4_cores),
        benchmark_fn("collisions", move || {
            (0..COUNT).into_par_iter().for_each(|_| {
                let element = random_aabb(10_000.0);

                tree.get_collisions(element, |elem| {
                    black_box(elem);
                });
            });
        }),
    ]
}

tango_benchmarks!(build_benchmarks());
tango_main!();
