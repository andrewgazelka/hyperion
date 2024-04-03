#![feature(allocator_api)]
use std::alloc::Global;

use divan::{AllocProfiler, Bencher};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

use bvh::{aabb::Aabb, Bvh, Element};
use rand::Rng;

fn create_element(min: [f32; 3], max: [f32; 3]) -> Element {
    Element {
        aabb: Aabb::new(min, max),
    }
}

fn create_random_elements(count: usize) -> Vec<Element> {
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

#[divan::bench]
fn bench_build_bvh(b: Bencher) {
    let mut elements = create_random_elements(100);

    b.bench_local(|| Bvh::build_in(&mut elements, Global));
}

#[divan::bench]
fn bench_query_bvh(b: Bencher) {
    let mut elements = create_random_elements(100);
    let bvh = Bvh::build_in(&mut elements, Global);

    let mut rng = rand::thread_rng();

    b.bench_local(|| {
        let min = [rng.gen_range(0.0..1000.0); 3];
        let max = [
            rng.gen_range(min[0]..1000.0),
            rng.gen_range(min[1]..1000.0),
            rng.gen_range(min[2]..1000.0),
        ];
        let target = Aabb::new(min, max);

        bvh.get_collisions(target).count()
    });
}
