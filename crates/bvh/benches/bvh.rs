use std::hint::black_box;

use divan::{AllocProfiler, Bencher};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    rayon::ThreadPoolBuilder::default()
        .build_global()
        .expect("Failed to build global thread pool");

    divan::main();
}

use bvh::{aabb::Aabb, Bvh, Element};
use rand::Rng;

fn create_element(min: [f32; 3], max: [f32; 3]) -> Element {
    Element {
        aabb: Aabb::new(min, max),
    }
}

fn create_random_elements_full(count: usize) -> Vec<Element> {
    let mut rng = rand::thread_rng();
    let mut elements = Vec::new();

    for _ in 0..count {
        let min = [rng.gen_range(0.0..1000.0); 3];
        let max = [
            rng.gen_range(min[0]..1000.0),
            rng.gen_range(min[1]..1000.0),
            rng.gen_range(min[2]..1000.0),
        ];
        elements.push(create_element(min, max));
    }

    elements
}

fn random_element_1() -> Element {
    let mut rng = rand::thread_rng();
    let min = [rng.gen_range(0.0..1000.0); 3];
    let max = [
        rng.gen_range(min[0]..1000.0),
        rng.gen_range(min[1]..1000.0),
        rng.gen_range(min[2]..1000.0),
    ];
    create_element(min, max)
}

fn create_random_elements_1(count: usize) -> Vec<Element> {
    let mut elements = Vec::new();

    for _ in 0..count {
        elements.push(random_element_1());
    }

    elements
}

#[divan::bench]
fn bench_build_bvh(b: Bencher) {
    let mut elements = create_random_elements_1(100_000);
    b.bench_local(|| Bvh::build(&mut elements));
}

const COUNTS: &[usize] = &[1, 10, 100, 1_000, 10_000, 100_000];

#[divan::bench(args = COUNTS)]
fn query(b: Bencher, count: usize) {
    let mut elements = create_random_elements_1(100_000);
    let bvh = Bvh::build(&mut elements);

    b.counter(count).bench(|| {
        for _ in 0..count {
            let element = random_element_1();
            for elem in bvh.get_collisions(element.aabb) {
                black_box(elem);
            }
        }
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
