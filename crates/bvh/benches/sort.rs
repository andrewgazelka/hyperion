use std::hint::black_box;

use criterion::{criterion_group, Criterion, Bencher, criterion_main};
use ordered_float::OrderedFloat;

const LEN: usize = 100;

fn sort_unwrap(bencher: &mut Bencher) {
    let mut arr = vec![0.0; LEN];
    bencher.iter(|| {
        // step 1: random arr of LEN, floats
        for elem in &mut arr {
            *elem = rand::random::<f32>();
        }
        arr.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

        black_box(&arr);
    });
}

fn sort_unwrap_unchecked(bencher: &mut Bencher) {
    let mut arr = vec![0.0; LEN];
    bencher.iter(|| {
        // step 1: random arr of LEN, floats
        for elem in &mut arr {
            *elem = rand::random::<f32>();
        }
        arr.sort_unstable_by(|a, b| unsafe { a.partial_cmp(b).unwrap_unchecked() });

        black_box(&arr);
    });
}

fn sort_unwrap_unchecked_key(bencher: &mut Bencher) {
    let mut arr = vec![0.0; LEN];
    bencher.iter(|| {
        // step 1: random arr of len, floats
        for elem in &mut arr {
            *elem = rand::random::<f32>();
        }
        arr.sort_unstable_by_key(|a| OrderedFloat(*a));

        black_box(&arr);
    });
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("sort_unwrap", sort_unwrap);
    c.bench_function("sort_unwrap_unchecked", sort_unwrap_unchecked);
    c.bench_function("sort_unwrap_unchecked_key", sort_unwrap_unchecked_key);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
