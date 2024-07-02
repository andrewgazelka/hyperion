use clap::Parser;
use hyperion_proxy::run_proxy;

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

    let params = Params::parse();

    run_proxy(params.proxy_addr, params.server_addr)
        .await
        .unwrap();
}
