use std::{
    net::SocketAddr,
    process::{Child, Command},
    time::{Duration, Instant},
};

use server::Game;

const PLAYER_COUNT: u32 = 100;

fn spawn_bots(port: u16) -> Child {
    let child = Command::new("rust-mc-bot")
        .arg(format!("127.0.0.1:{port}"))
        .arg(PLAYER_COUNT.to_string())
        .spawn()
        .expect("failed to start worker process");

    child
}

#[test]
fn test_100_players() {
    const MS_PER_TICK: u64 = 50;
    const NUM_TICKS: u64 = 200; // 10 seconds

    // init tracing
    tracing_subscriber::fmt().try_init().unwrap();

    let port = 25565;

    let threads = 4;

    let thread_pool = rayon::ThreadPoolBuilder::default()
        .num_threads(threads)
        .build()
        .expect("Failed to build global thread pool");

    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let mut game = Game::init(addr).unwrap();

    let mut handle = spawn_bots(port);

    let delay = Duration::from_millis(MS_PER_TICK);

    let start = Instant::now();

    thread_pool.install(|| {
        // simulate 1000 ticks
        for _ in 0..NUM_TICKS {
            game.tick();
            spin_sleep::sleep(delay);
        }
    });

    let elapsed = start.elapsed();

    handle.kill().unwrap();
    game.shutdown();

    println!("elapsed: {elapsed:?}");

    let time_pure_delay = delay * NUM_TICKS as u32;
    let time_processing = elapsed - time_pure_delay;

    let average_mspt = time_processing.as_millis() as f64 / NUM_TICKS as f64;

    println!("average mspt: {average_mspt}ms");
}
