use hyperion_proxy::run_proxy;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    run_proxy("127.0.0.1:25566", "127.0.0.1:25565")
        .await
        .unwrap();
}
