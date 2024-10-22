use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;
use hyperion_proxy::run_proxy;
use tokio::net::{TcpListener, UnixListener};

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
    let params = Params::parse();

    let proxy_addr = ProxyAddress::parse(&params.proxy_addr)?;
    let server_addr: SocketAddr = params.server_addr.parse()?;

    let handle = tokio::task::Builder::new()
        .name("proxy")
        .spawn(async move {
            match proxy_addr {
                ProxyAddress::Tcp(addr) => {
                    run_proxy(TcpListener::bind(addr).await.unwrap(), server_addr)
                        .await
                        .unwrap();
                }
                ProxyAddress::Unix(path) => {
                    run_proxy(UnixListener::bind(path).unwrap(), server_addr)
                        .await
                        .unwrap();
                }
            }
        })
        .unwrap();

    handle.await?;
    Ok(())
}
