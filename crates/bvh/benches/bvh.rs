use std::hint::black_box;

use divan::{AllocProfiler, Bencher};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

use bvh::{create_random_elements_1, random_element_1, Bvh, Heuristic, TrivialHeuristic};

const ENTITY_COUNTS: &[usize] = &[1, 10, 100, 1_000, 10_000, 100_000];

#[divan::bench(
    args = ENTITY_COUNTS,
    types = [TrivialHeuristic],
)]
fn build<H: Heuristic>(b: Bencher, count: usize) {
    let mut elements = create_random_elements_1(count);
    b.counter(count)
        .bench_local(|| Bvh::build::<H>(&mut elements));
}

#[divan::bench(
    args = ENTITY_COUNTS,
    types = [TrivialHeuristic],
)]
fn query<T: Heuristic>(b: Bencher, count: usize) {
    let mut elements = create_random_elements_1(count);
    let bvh = Bvh::build::<T>(&mut elements);

    b.counter(count).bench_local(|| {
        for _ in 0..count {
            let element = random_element_1();
            bvh.get_collisions(element, |elem| {
                black_box(elem);
            });
        }
    });
}

#[divan::bench(
    args = ENTITY_COUNTS,
    threads,
    types = [TrivialHeuristic],
)]
fn query_par<T: Heuristic>(b: Bencher, count: usize) {
    let mut elements = create_random_elements_1(100_000);
    let bvh = Bvh::build::<T>(&mut elements);

    b.counter(count).bench(|| {
        for _ in 0..count {
            let element = random_element_1();
            bvh.get_collisions(element, |elem| {
                black_box(elem);
            });
        }
    });
}

const THREAD_COUNTS: &[usize] = &[1, 2, 4, 8];

#[divan::bench(
    args = THREAD_COUNTS,
    types = [TrivialHeuristic],
)]
fn build_100k_rayon<T: Heuristic>(b: Bencher, count: usize) {
    let thread_pool = rayon::ThreadPoolBuilder::default()
        .num_threads(count)
        .build()
        .expect("Failed to build global thread pool");

    let count: usize = 100_000;

    let elements = create_random_elements_1(count);

    b.counter(count).bench(|| {
        thread_pool.install(|| {
            let mut elements = elements.clone();
            Bvh::build::<T>(&mut elements);
        });
    });
}

// #[divan::bench]
// fn bench_query_bvh(b: Bencher) {
//     let mut elements = create_random_elements(100);
//     let bvh = Bvh::build_in(&mut elements, Global);
//
//     let mut rng = rand::thread_rng();
//
//     b.bench_local(|| {
//         let min = [rng.gen_range(0.0..1000.0); 3];
//         let max = [
//             rng.gen_range(min[0]..1000.0),
//             rng.gen_range(min[1]..1000.0),
//             rng.gen_range(min[2]..1000.0),
//         ];
//         let target = Aabb::new(min, max);
//
//         bvh.get_collisions(target).count()
//     });
// }
