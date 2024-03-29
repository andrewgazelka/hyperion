use std::{collections::VecDeque, hint::black_box};

use broadcast::utils::group::group;
use divan::{AllocProfiler, Bencher};
use rand::prelude::SliceRandom;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

// say render distance is 32 chunks
// this means we have 32 * 32 = 1024 chunks we will have to go through
const LENS: &[u32] = &[64, 128, 256, 512, 1024, 2048];

// Register a `fibonacci` function and benchmark it over multiple cases.
#[divan::bench(
    args = LENS,
)]
fn contiguous(bencher: Bencher, len: u32) {
    let v = (0..len).collect::<Vec<_>>();

    bencher.counter(len).bench(|| {
        group(&v).for_each(|elem| {
            black_box(elem);
        });
    });
}

#[divan::bench(
    args = LENS,
)]
fn random_eighth(bencher: Bencher, len: u32) {
    let mut v: Vec<_> = (0..len).collect();

    // shuffle using rand
    v.shuffle(&mut rand::thread_rng());

    let eigth = len / 8;

    let idxes = &mut v[..eigth as usize];

    // sort
    idxes.sort_unstable();

    let idxes = &v[..eigth as usize];

    bencher.counter(len).bench(|| {
        group(idxes).for_each(|elem| {
            black_box(elem);
        });
    });
}
