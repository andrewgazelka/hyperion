use clap::Parser;
use infection::init_game;

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

fn main() {
    tracing_subscriber::fmt::init();

    let Args { ip, port } = Args::parse();

    let address = format!("{ip}:{port}");

    init_game(address).unwrap();
}
