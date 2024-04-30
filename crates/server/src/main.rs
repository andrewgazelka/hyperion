//! The main entry point for the server.

use clap::Parser;
use server::Hyperion;

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

    let mut game = Hyperion::init(default_address)?;
    game.game_loop();
    Ok(())
}
