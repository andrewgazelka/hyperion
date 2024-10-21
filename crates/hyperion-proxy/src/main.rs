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
    console_subscriber::init();

    let params = Params::parse();

    let handle = tokio::task::Builder::new()
        .name("proxy")
        .spawn(async move {
            run_proxy(params.proxy_addr, params.server_addr)
                .await
                .unwrap();
        })
        .unwrap();

    handle.await.unwrap();
}
