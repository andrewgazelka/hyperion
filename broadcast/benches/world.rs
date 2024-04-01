#![feature(thread_local)]

use std::{cell::RefCell, collections::BTreeMap, hint::black_box};

use broadcast::Broadcaster;
use divan::Bencher;

// #[global_allocator]
// static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

// say render distance is 32 chunks
// this means we have 32 * 32 = 1024 chunks we will have to go through
// const LENS: &[u16] = &[1, 2, 4, 8, 16, 32, 64];
const LENS: &[u16] = &[128, 256, 512, 1024];

#[inline(never)]
fn generate_world(width: u16) -> Broadcaster {
    let mut world = Broadcaster::create(width).unwrap();

    world.populate(|_, data| {
        let data_len = rand::random::<u8>();
        let to_push = (0..data_len)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<_>>();

        data.extend(to_push);
    });

    world
}

// for normal (x, y) -> x + y * width
// world                         fastest       │ slowest       │ median        │ mean          │
// samples │ iters ╰─ player_render_distance_32                │               │               │
// │         │    ├─ 128                     709.6 µs      │ 1.138 ms      │ 785.8 µs      │ 827.8
// µs      │ 100     │ 100    │                          14.09 Mitem/s │ 8.78 Mitem/s  │ 12.72
// Mitem/s │ 12.07 Mitem/s │         │    ├─ 256                     835.8 µs      │ 1.078 ms      │
// 937.8 µs      │ 935.2 µs      │ 100     │ 100    │                          11.96 Mitem/s │ 9.268
// Mitem/s │ 10.66 Mitem/s │ 10.69 Mitem/s │         │    ├─ 512                     1.004 ms      │
// 1.319 ms      │ 1.124 ms      │ 1.114 ms      │ 100     │ 100    │                          9.952
// Mitem/s │ 7.576 Mitem/s │ 8.893 Mitem/s │ 8.969 Mitem/s │         │    ╰─ 1024
// 1.037 ms      │ 1.336 ms      │ 1.132 ms      │ 1.138 ms      │ 100     │ 100
// 9.639 Mitem/s │ 7.48 Mitem/s  │ 8.833 Mitem/s │ 8.784 Mitem/s │         │

//

#[thread_local]
static WORLDS: RefCell<BTreeMap<u16, Broadcaster>> = RefCell::new(BTreeMap::new());

// Register a `fibonacci` function and benchmark it over multiple cases.
#[divan::bench(
    args = LENS,
)]
fn player_render_distance_32(bencher: Bencher, len: u16) {
    const PLAYER_COUNT: usize = 10_000;
    const PLAYER_VIEW_DISTANCE: u16 = 32;

    let mut worlds = WORLDS.borrow_mut();
    let world = worlds.entry(len).or_insert_with(|| generate_world(len));

    // let mut world = generate_world(len);

    bencher.counter(PLAYER_COUNT).bench_local(move || {
        for _ in 0..PLAYER_COUNT {
            let start_x = rand::random::<u16>() % (len - PLAYER_VIEW_DISTANCE * 2);
            let end_x = start_x + PLAYER_VIEW_DISTANCE * 2;

            let start_y = rand::random::<u16>() % (len - PLAYER_VIEW_DISTANCE * 2);
            let end_y = start_y + PLAYER_VIEW_DISTANCE * 2;

            let res = world.data_range(start_x..end_x, start_y..end_y);
            for elem in res {
                black_box(elem);
            }
        }
    });
}
