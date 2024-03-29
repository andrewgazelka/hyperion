use divan::{AllocProfiler, Bencher};
use rand::prelude::SliceRandom;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

// Register a `fibonacci` function and benchmark it over multiple cases.

const LENS: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

#[divan::bench(
    args = LENS,
)]
fn rebuild_single(bencher: Bencher, len: usize) {
    let v = (0..len).collect::<Vec<_>>();

    let changes = vec![quadtree::rebuild::MoveElement {
        remove_from_idx: 1,
        insert_to_idx: 3,
    }];

    bencher.bench(|| {
        quadtree::rebuild::apply_vec(&v, &changes, &mut [0, 1, 2, 3, 4]);
    });
}

#[divan::bench(
args = LENS,
)]
fn rebuild_every(bencher: Bencher, len: usize) {
    let v = (0..len).collect::<Vec<_>>();

    let arr = 0..len;

    // shuffle the array
    let mut arr = arr.collect::<Vec<_>>();
    arr.shuffle(&mut rand::thread_rng());

    let changes = arr
        .iter()
        .enumerate()
        .map(|(i, &x)| quadtree::rebuild::MoveElement {
            remove_from_idx: i,
            insert_to_idx: x,
        })
        .collect::<Vec<_>>();

    bencher.counter(len).bench(|| {
        quadtree::rebuild::apply_vec(&v, &changes, &mut [0, 1, 2, 3, 4]);
    });
}

// not very helpful and makes results more confusing
// #[divan::bench(
// args = LENS,
// )]
// fn rebuild_4096(bencher: Bencher, len: usize) {
//     let v = (0..4096).collect::<Vec<_>>();
//
//     let arr = 0..len;
//
//     // shuffle the array
//     let mut arr = arr.collect::<Vec<_>>();
//     arr.shuffle(&mut rand::thread_rng());
//
//     let changes = arr
//         .iter()
//         .enumerate()
//         .map(|(i, &x)| quadtree::rebuild::MoveElement {
//             remove_from_idx: i,
//             insert_to_idx: x,
//         })
//         .collect::<Vec<_>>();
//
//     bencher.counter(4096_usize).bench(|| {
//         quadtree::rebuild::apply_vec(&v, &changes, &mut [0, 1, 2, 3, 4]);
//     });
// }
