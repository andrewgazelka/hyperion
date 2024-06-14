use clap::Parser;
use server::Hyperion;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter};

/// The arguments to run the server
#[derive(Parser)]
struct Args {
    /// The IP address the server should listen on. Defaults to 0.0.0.0
    #[clap(short, long, default_value = "0.0.0.0")]
    ip: String,
    /// The port the server should listen on. Defaults to 25565
    #[clap(short, long, default_value = "25565")]
    port: u16,

    #[clap(short, long, default_value = "false")]
    tracy: bool,
}

fn main() {
    let Args { ip, port, tracy } = Args::parse();

    if tracy {
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(tracing_tracy::TracyLayer::default()),
        )
        .expect("setup tracy layer");
    } else {
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
            .expect("setup tracing");
    }

    let address = format!("{ip}:{port}");

    Hyperion::init(address).unwrap();
}
