// https://bheisler.github.io/criterion.rs/book/faq.html#cargo-bench-gives-unrecognized-option-errors-for-valid-command-line-options
// https://github.com/bheisler/iai/issues/37
// https://github.com/osiewicz/calliper
// https://github.com/iai-callgrind/iai-callgrind
// https://nikolaivazquez.com/blog/divan/#measure-allocations

use divan::Bencher;
// use thread_priority::{ThreadBuilderExt, ThreadPriority};
use server::{bounding_box::BoundingBox, FullEntityPose, Game, InitEntity, Targetable};
use valence_protocol::math::DVec3;

fn main() {
    divan::main();
}

const THREAD_COUNTS: &[usize] = &[1, 2, 4, 8];

#[divan::bench(
    args = THREAD_COUNTS,
    sample_count = 1,
)]
fn world_bench(bencher: Bencher, thread_count: usize) {
    // so we can have reliable benchmarks even when we are using our laptop for other
    // things

    // Run registered benchmarks.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .unwrap();

    const TICKS: usize = 100;

    let mut game = Game::init().unwrap();

    let count: usize = 100_000;

    const BASE_RADIUS: f64 = 4.0;

    // normalize over the number
    let radius = BASE_RADIUS * (count as f64).sqrt();

    let loc = DVec3::new(0.0, 10.0, 0.0);

    for _ in 0..count {
        // spawn in 100 block radius
        let x = (rand::random::<f64>() - 0.5).mul_add(radius, loc.x);
        let y = loc.y;
        let z = (rand::random::<f64>() - 0.5).mul_add(radius, loc.z);

        game.world_mut().send(InitEntity {
            pose: FullEntityPose {
                position: DVec3::new(x, y, z),
                yaw: 0.0,
                pitch: 0.0,
                bounding: BoundingBox::create(DVec3::new(x, y, z), 0.6, 1.8),
            },
        });
    }

    game.tick();

    let world = game.world_mut();

    let id = world.spawn();

    world.insert(id, FullEntityPose {
        position: DVec3::new(0.0, 2.0, 0.0),
        bounding: BoundingBox::create(DVec3::new(0.0, 2.0, 0.0), 0.6, 1.8),
        yaw: 0.0,
        pitch: 0.0,
    });

    world.insert(id, Targetable);

    bencher.counter(TICKS).bench_local(|| {
        pool.install(|| {
            for _ in 0..TICKS {
                game.tick();
            }
        });
    });
}
