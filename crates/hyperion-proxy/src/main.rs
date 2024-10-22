use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;
use hyperion_proxy::run_proxy;
use tokio::net::{TcpListener, UnixListener};
use tracing::{error, info};

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Parser)]
struct Params {
    #[clap(default_value = "0.0.0.0:25565")]
    proxy_addr: String,

    #[clap(short, long, default_value = "127.0.0.1:35565")]
    server_addr: String,
}

#[derive(Debug)]
enum ProxyAddress {
    Tcp(SocketAddr),
    Unix(PathBuf),
}

impl ProxyAddress {
    fn parse(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        if addr.contains(':') {
            Ok(Self::Tcp(addr.parse()?))
        } else {
            Ok(Self::Unix(PathBuf::from(addr)))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let params = Params::parse();

    let proxy_addr = ProxyAddress::parse(&params.proxy_addr)?;
    let server_addr: SocketAddr = params.server_addr.parse()?;

    info!("Starting Hyperion Proxy");
    info!("Proxy address: {proxy_addr:?}");
    info!("Server address: {}", server_addr);

    let handle = tokio::task::Builder::new()
        .name("proxy")
        .spawn(async move {
            match &proxy_addr {
                ProxyAddress::Tcp(addr) => {
                    info!("Binding to TCP address: {}", addr);
                    run_proxy(TcpListener::bind(addr).await.unwrap(), server_addr)
                        .await
                        .unwrap();
                }
                ProxyAddress::Unix(path) => {
                    info!("Binding to Unix socket: {:?}", path);
                    run_proxy(UnixListener::bind(path).unwrap(), server_addr)
                        .await
                        .unwrap();
                }
            }
        })
        .unwrap();

    if let Err(e) = handle.await {
        error!("Proxy task failed: {:?}", e);
    } else {
        info!("Proxy task completed successfully");
    }
    Ok(())
}
