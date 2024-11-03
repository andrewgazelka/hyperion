use clap::Parser;
use proof_of_concept::init_game;
use tracing_subscriber::layer::SubscriberExt;
use tracing_tracy::TracyLayer;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// The arguments to run the server
#[derive(Parser)]
struct Args {
    /// The IP address the server should listen on. Defaults to 0.0.0.0
    #[clap(short, long, default_value = "0.0.0.0")]
    ip: String,
    /// The port the server should listen on. Defaults to 25565
    #[clap(short, long, default_value = "35565")]
    port: u16,
}

fn setup_logging() {
    // // // Build a custom subscriber
    // // tracing_subscriber::fmt()
    // //     .with_ansi(true)
    // //     .with_file(false)
    // //     .with_line_number(false)
    // //     .with_target(false)
    // //     .with_max_level(tracing::Level::DEBUG)
    // //     .with_env_filter(EnvFilter::from_default_env())
    // //     .init();
    //
    // tracing::subscriber::set_global_default(
    //     tracing_subscriber::registry().with(tracing_tracy::TracyLayer::default()),
    // )
    // .expect("setup tracy layer");

    // tracing::subscriber::set_global_default(
    //     tracing_subscriber::registry().with(
    //         TracyLayer::default()
    //             .with_filter(tracing::Level::DEBUG)
    //     )
    // ).expect("setup tracy layer");
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(TracyLayer::default())
            // .with(tracing_subscriber::filter::LevelFilter::INFO),
    )
    .expect("setup tracy layer");
}

fn main() {
    dotenvy::dotenv().ok();

    setup_logging();

    // console_subscriber::init();

    let Args { ip, port } = Args::parse();

    let address = format!("{ip}:{port}");

    // Denormals (numbers very close to 0) are flushed to zero because doing computations on them
    // is slow.
    init_game(address).unwrap();
}
