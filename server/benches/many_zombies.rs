use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use server::{bounding_box::BoundingBox, FullEntityPose, Game, InitEntity};
use valence_protocol::math::DVec3;

fn criterion_benchmark(c: &mut Criterion) {
    // so we can have reliable benchmarks even when we are using our laptop for other 
    // things
    rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build_global()
        .unwrap();

    let mut game = Game::init().unwrap();

    let count = 100_000;

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

    // just a tick to setup
    game.tick();

    c.bench_function("world", |b| b.iter(|| game.tick()));
}

criterion_group! {
  name = benches;
  config = Criterion::default().measurement_time(Duration::from_secs(20));
  targets = criterion_benchmark
}

// criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
