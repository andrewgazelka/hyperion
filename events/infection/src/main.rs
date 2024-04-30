use clap::Parser;
use infection::init_game;
use tracing_subscriber::EnvFilter;

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
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        // .pretty()
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
            "%H:%M:%S %3fms".to_owned(),
        ))
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .try_init()
        .unwrap();

    let Args { ip, port } = Args::parse();

    let address = format!("{ip}:{port}");

    // Denormals (numbers very close to 0) are flushed to zero because doing computations on them
    // is slow.
    init_game(address).unwrap();
}
