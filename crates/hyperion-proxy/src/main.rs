use clap::Parser;
use hyperion_proxy::run_proxy;
use jemallocator::Jemalloc;

#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Parser)]
struct Params {
    #[clap(short, long, default_value = "0.0.0.0:25565")]
    proxy_addr: String,

    #[clap(short, long, default_value = "127.0.0.1:35565")]
    server_addr: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // let client = tracy_client::Client::start();
    // tracy_client::plot!("test", 0.0);
    // client.plot(tracy_client::plot!("test", 0.0));
    // let tracy = Tracy::default(),

    // tracing::subscriber::set_global_default(
    //     tracing_subscriber::registry().with(tracing_tracy::TracyLayer::default()),
    // )
    // .expect("setup tracy layer");

    // tracing_subscriber::fmt()
    //     .with_span_events(FmtSpan::CLOSE)
    //     .with_target(false)
    //     // .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
    //     .with_max_level(Level::INFO)
    //     .with_level(false)
    //     .init();

    // let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    //
    // tracing_subscriber::fmt()
    //     .with_env_filter(filter)
    //     // .pretty()
    //     .with_timer(tracing_subscriber::fmt::time::ChronoLocal::new(
    //         "%H:%M:%S %3fms".to_owned(),
    //     ))
    //     .with_file(false)
    //     .with_line_number(false)
    //     .with_target(false)
    //     .try_init()
    //     .expect("setup tracing");

    let params = Params::parse();

    let handle = tokio::spawn(async move {
        run_proxy(params.proxy_addr, params.server_addr)
            .await
            .unwrap();
    });

    handle.await.unwrap();
}
