use std::{collections::VecDeque, fs::File, hint::black_box};

use broadcast::{utils::group::group, World};
use divan::{AllocProfiler, Bencher};
use rand::prelude::SliceRandom;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

// say render distance is 32 chunks
// this means we have 32 * 32 = 1024 chunks we will have to go through
// const LENS: &[u16] = &[1, 2, 4, 8, 16, 32, 64];
const LENS: &[u16] = &[128, 256, 512, 1024];

#[inline(never)]
fn generate_world(width: u16) -> World {
    let mut world = World::create(width);

    world.populate(|_, data| {
        let data_len = rand::random::<u8>();
        let to_push = (0..data_len)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<_>>();

        data.extend(to_push);
    });

    world
}

// Register a `fibonacci` function and benchmark it over multiple cases.
#[divan::bench(
    args = LENS,
)]
fn player_render_distance_32(bencher: Bencher, len: u16) {
    const PLAYER_COUNT: usize = 10_000;
    const PLAYER_VIEW_DISTANCE: u16 = 32;

    let mut world = generate_world(len);
    world.data_range(0..len, 0..len);

    bencher.counter(PLAYER_COUNT).bench_local(move || {
        for _ in 0..PLAYER_COUNT {
            let start_x = rand::random::<u16>() % (len - PLAYER_VIEW_DISTANCE * 2);
            let end_x = start_x + PLAYER_VIEW_DISTANCE * 2;

            let start_y = rand::random::<u16>() % (len - PLAYER_VIEW_DISTANCE * 2);
            let end_y = start_y + PLAYER_VIEW_DISTANCE * 2;

            let res = world.data_range(start_x..end_x, start_y..end_y);
            black_box(res);
        }
    });

    // if let Ok(report) = guard.report().build() {
    //     let file = File::create("flamegraph.svg").unwrap();
    //     report.flamegraph(file).unwrap();
    // };
}
