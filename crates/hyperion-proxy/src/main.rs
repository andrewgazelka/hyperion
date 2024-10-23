use std::{fmt::Debug, net::SocketAddr, path::PathBuf};

use clap::Parser;
use hyperion_proxy::run_proxy;
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

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
    #[cfg(unix)]
    Unix(PathBuf),
}

use std::fmt::Display;

use colored::Colorize;

impl Display for ProxyAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp(addr) => write!(f, "tcp://{addr}"),
            #[cfg(unix)]
            Self::Unix(path) => write!(f, "unix://{}", path.display()),
        }
    }
}

impl ProxyAddress {
    fn parse(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        if addr.contains(':') {
            Ok(Self::Tcp(addr.parse()?))
        } else {
            #[cfg(unix)]
            {
                Ok(Self::Unix(PathBuf::from(addr)))
            }
            #[cfg(not(unix))]
            {
                Err("Unix sockets are not supported on this platform".into())
            }
        }
    }
}

fn setup_logging() {
    // Build a custom subscriber
    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .with_env_filter(EnvFilter::from_default_env())
        .with_max_level(tracing::Level::INFO)
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();
    let params = Params::parse();

    let proxy_addr = ProxyAddress::parse(&params.proxy_addr)?;
    let server_addr: SocketAddr = params.server_addr.parse()?;

    let login_help = "~ The address to connect to".dimmed();

    info!("Starting Hyperion Proxy");
    info!("📡 Public proxy address: {proxy_addr} {login_help}",);

    let server_help = "~ The event server internal address".dimmed();
    info!("👾 Internal server address: tcp://{server_addr} {server_help}");

    let handle = tokio::task::Builder::new()
        .name("proxy")
        .spawn(async move {
            match &proxy_addr {
                ProxyAddress::Tcp(addr) => {
                    let socket = TcpListener::bind(addr).await.unwrap();
                    run_proxy(socket, server_addr).await.unwrap();
                }
                #[cfg(unix)]
                ProxyAddress::Unix(path) => {
                    let socket = UnixListener::bind(path).unwrap();
                    run_proxy(socket, server_addr).await.unwrap();
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
