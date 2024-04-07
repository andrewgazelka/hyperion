use std::hint::black_box;

use divan::{AllocProfiler, Bencher};
use ordered_float::OrderedFloat;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

const ARR_LENGTH: &[usize] = &[32768, 65536];

#[divan::bench(args=ARR_LENGTH)]
fn sort_unwrap(bencher: Bencher, len: usize) {
    let mut arr = vec![0.0; len];
    bencher.bench_local(|| {
        // step 1: random arr of len, floats
        for elem in &mut arr {
            *elem = rand::random::<f32>();
        }
        arr.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

        black_box(&arr);
    });
}

#[divan::bench(args =ARR_LENGTH)]
fn sort_unwrap_unchecked(bencher: Bencher, len: usize) {
    let mut arr = vec![0.0; len];
    bencher.bench_local(|| {
        // step 1: random arr of len, floats
        for elem in &mut arr {
            *elem = rand::random::<f32>();
        }
        arr.sort_unstable_by(|a, b| unsafe { a.partial_cmp(b).unwrap_unchecked() });

        black_box(&arr);
    });
}

#[divan::bench(args =ARR_LENGTH)]
fn sort_unwrap_unchecked_key(bencher: Bencher, len: usize) {
    let mut arr = vec![0.0; len];
    bencher.bench_local(|| {
        // step 1: random arr of len, floats
        for elem in &mut arr {
            *elem = rand::random::<f32>();
        }
        arr.sort_unstable_by_key(|a| OrderedFloat(*a));

        black_box(&arr);
    });
}
