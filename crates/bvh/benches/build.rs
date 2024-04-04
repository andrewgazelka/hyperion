use std::hint::black_box;

use bvh::{aabb::Aabb, create_random_elements_1, random_aabb, Bvh, TrivialHeuristic};
use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};

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
    let count: usize = 1_000_000;
    let elements = create_random_elements_1(count, 10_000.0);
    let mut elements = elements;
    Bvh::build::<TrivialHeuristic>(&mut elements)
}

fn build_benchmarks() -> impl IntoBenchmarks {
    let tree = build_tree();

    [
        benchmark_fn("build", benchmark_1m_4_cores),
        benchmark_fn("collisions", move || {
            for _ in 0..1000 {
                let element = random_aabb(10_000.0);
                let mut vec = Vec::new();

                tree.get_collisions(element, |elem| {
                    vec.push(*elem);
                });

                black_box(vec);
            }
        }),
    ]
}

tango_benchmarks!(build_benchmarks());
tango_main!();
