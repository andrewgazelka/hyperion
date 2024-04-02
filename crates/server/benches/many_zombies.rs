// https://bheisler.github.io/criterion.rs/book/faq.html#cargo-bench-gives-unrecognized-option-errors-for-valid-command-line-options
// https://github.com/bheisler/iai/issues/37
// https://github.com/osiewicz/calliper
// https://github.com/iai-callgrind/iai-callgrind
// https://nikolaivazquez.com/blog/divan/#measure-allocations

#![feature(lint_reasons)]

use divan::Bencher;
use server::{bounding_box::BoundingBox, FullEntityPose, Game, InitEntity, Targetable};
use valence_protocol::math::Vec3;

fn main() {
    divan::main();
}

const THREAD_COUNTS: &[usize] = &[1, 2, 4, 8];

#[divan::bench(
    args = THREAD_COUNTS,
    sample_count = 1,
)]
fn world_bench(bencher: Bencher, thread_count: usize) {
    const TICKS: usize = 100;
    const BASE_RADIUS: f32 = 4.0;

    // so we can have reliable benchmarks even when we are using our laptop for other
    // things

    // Run registered benchmarks.
    #[expect(clippy::unwrap_used, reason = "this is a bench")]
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .unwrap();

    #[expect(clippy::unwrap_used, reason = "this is a bench")]
    let mut game = Game::init().unwrap();

    let count: u32 = 100_000;

    // normalize over the number

    #[expect(clippy::cast_possible_truncation, reason = "sqrt of f64 is f32")]
    let radius = BASE_RADIUS * f64::from(count).sqrt() as f32;

    let loc = Vec3::new(0.0, 10.0, 0.0);

    for _ in 0..count {
        // spawn in 100 block radius
        let x = (rand::random::<f32>() - 0.5).mul_add(radius, loc.x);
        let y = loc.y;
        let z = (rand::random::<f32>() - 0.5).mul_add(radius, loc.z);

        game.world_mut().send(InitEntity {
            pose: FullEntityPose {
                position: Vec3::new(x, y, z),
                yaw: 0.0,
                pitch: 0.0,
                bounding: BoundingBox::create(Vec3::new(x, y, z), 0.6, 1.8),
            },
        });
    }

    game.tick();

    let world = game.world_mut();

    let id = world.spawn();

    world.insert(id, FullEntityPose {
        position: Vec3::new(0.0, 2.0, 0.0),
        bounding: BoundingBox::create(Vec3::new(0.0, 2.0, 0.0), 0.6, 1.8),
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
