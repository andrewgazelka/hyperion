use std::{
    net::{SocketAddr, TcpListener},
    process::{Child, Command},
    sync::atomic::AtomicU16,
};

use server::Game;
use tango_bench::{benchmark_fn, tango_benchmarks, tango_main, IntoBenchmarks};


fn spawn_bots(port: u16) -> Child {
    // no stdio
    let child = Command::new("rust-mc-bot")
        .arg(format!("127.0.0.1:{port}"))
        .arg(PLAYER_COUNT.to_string())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("failed to start worker process");

    child
}

static PORT: AtomicU16 = AtomicU16::new(25565);

fn bench_100_players() {
    let threads = 4;
    let thread_pool = rayon::ThreadPoolBuilder::default()
        .num_threads(threads)
        .build()
        .expect("Failed to build global thread pool");

    let port = PORT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let mut game = Game::init(addr).unwrap();

    let mut handle = spawn_bots(port);

    thread_pool.install(|| {
        // simulate 1000 ticks
        for _ in 0..10_000 {
            game.tick();
        }
    });

    handle.kill().unwrap();
    game.shutdown();

}

fn build_benchmarks() -> impl IntoBenchmarks {
    [benchmark_fn("bench_100_players", bench_100_players)]
}

tango_benchmarks!(build_benchmarks());
tango_main!();
