use std::hint::black_box;

use divan::{AllocProfiler, Bencher};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

use bvh::{create_random_elements_1, random_aabb, Bvh, Heuristic, TrivialHeuristic};

const ENTITY_COUNTS: &[usize] = &[100, 1_000, 10_000];

#[divan::bench(
    args = ENTITY_COUNTS,
    types = [TrivialHeuristic],
)]
fn build<H: Heuristic>(b: Bencher, count: usize) {
    let mut elements = create_random_elements_1(count, 100.0);
    b.counter(count)
        .bench_local(|| Bvh::build::<H>(&mut elements));
}

#[divan::bench(
    args = ENTITY_COUNTS,
    types = [TrivialHeuristic],
)]
fn query<T: Heuristic>(b: Bencher, count: usize) {
    let mut elements = create_random_elements_1(count, 100.0);
    let bvh = Bvh::build::<T>(&mut elements);

    b.counter(count).bench_local(|| {
        for _ in 0..count {
            let element = random_aabb(100.0);
            bvh.get_collisions(element, |elem| {
                black_box(elem);
            });
        }
    });
}

#[divan::bench(
    args = ENTITY_COUNTS,
    types = [TrivialHeuristic],
)]
fn query_par<T: Heuristic>(b: Bencher, count: usize) {
    let mut elements = create_random_elements_1(100_000, 100.0);
    let bvh = Bvh::build::<T>(&mut elements);

    b.counter(count).bench_local(|| {
        (0..count).into_par_iter().for_each(|_| {
            let element = random_aabb(100.0);
            bvh.get_collisions(element, |elem| {
                black_box(elem);
            });
        })
    });
}

const THREAD_COUNTS: &[usize] = &[1, 2, 4, 8];

#[divan::bench(
    args = THREAD_COUNTS,
    types = [TrivialHeuristic],
)]
fn build_10k_rayon<T: Heuristic>(b: Bencher, count: usize) {
    let thread_pool = rayon::ThreadPoolBuilder::default()
        .num_threads(count)
        .build()
        .expect("Failed to build global thread pool");

    let count: usize = 10_000;

    let elements = create_random_elements_1(count, 100.0);

    b.counter(count).bench(|| {
        thread_pool.install(|| {
            let mut elements = elements.clone();
            Bvh::build::<T>(&mut elements);
        });
    });
}
