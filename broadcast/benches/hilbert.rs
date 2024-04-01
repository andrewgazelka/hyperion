use std::hint::black_box;

use broadcast::utils::{cache::hilbert::HilbertCache, group::group};
use divan::{AllocProfiler, Bencher};
use rand::prelude::SliceRandom;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

// this means we have 32 * 32 = 1024 chunks we will have to go through
//                       4           ..            1M
const ORDERS: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

#[divan::bench(
    args = ORDERS,
)]
fn get_sequential(bencher: Bencher, order: u8) {
    let width = 1 << order;

    let hilbert = HilbertCache::build(order).unwrap();

    let area = u64::from(width) * u64::from(width);

    bencher.counter(area).bench(|| {
        for x in 0..width {
            for y in 0..width {
                black_box(hilbert.get_hilbert(x, y));
            }
        }
    });
}

#[divan::bench(
    args = ORDERS,
)]
fn get_random(bencher: Bencher, order: u8) {
    let width = 1 << order;

    let hilbert = HilbertCache::build(order).unwrap();

    let area = u64::from(width) * u64::from(width);

    let mut rng = rand::thread_rng();

    let mut coords = (0..width)
        .flat_map(|x| (0..width).map(move |y| (x, y)))
        .collect::<Vec<_>>();

    coords.shuffle(&mut rng);

    bencher.counter(area).bench(|| {
        for (x, y) in coords.iter().copied() {
            black_box(hilbert.get_hilbert(x, y));
        }
    });
}
