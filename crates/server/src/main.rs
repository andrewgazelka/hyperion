//! The main entry point for the server.

use anyhow::Context;
use clap::Parser;
use server::{adjust_file_limits, Game};

mod tracing_utils;

fn main() -> anyhow::Result<()> {
    tracing_utils::with_tracing(init)
}

/// The arguments to run the server
#[derive(Parser)]
struct Args {
    /// The IP address the server should listen on. Defaults to 0.0.0.0
    #[clap(short, long, default_value = "0.0.0.0")]
    ip: String,
    /// The port the server should listen on. Defaults to 25565
    #[clap(short, long, default_value = "25565")]
    port: u16,
}

/// Initializes the server.
fn init() -> anyhow::Result<()> {
    let Args { ip, port } = Args::parse();

    let default_address = format!("{ip}:{port}");

    // 10k players, we want at least 2^14 = 16,384 file handles
    adjust_file_limits(16_384)?;

    rayon::ThreadPoolBuilder::new()
        .spawn_handler(|thread| {
            std::thread::spawn(|| no_denormals::no_denormals(|| thread.run()));
            Ok(())
        })
        .build_global()
        .context("failed to build thread pool")?;

    no_denormals::no_denormals(|| {
        let mut game = Game::init(default_address)?;
        game.game_loop();
        Ok(())
    })
}
