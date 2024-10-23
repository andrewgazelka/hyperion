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

use std::{
    fmt::Display,
    task::{Context, Poll},
};

use colored::Colorize;
use tokio_util::net::Listener;

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
                    let listener = TcpListener::bind(addr).await.unwrap();
                    let socket = NoDelayTcp { listener };
                    run_proxy(socket, server_addr).await.unwrap();
                }
                #[cfg(unix)]
                ProxyAddress::Unix(path) => {
                    let listener = UnixListener::bind(path).unwrap();
                    run_proxy(listener, server_addr).await.unwrap();
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

struct NoDelayTcp {
    listener: TcpListener,
}

impl Listener for NoDelayTcp {
    type Addr = <TcpListener as Listener>::Addr;
    type Io = <TcpListener as Listener>::Io;

    fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<(Self::Io, Self::Addr)>> {
        let Poll::Ready(result) = self.listener.poll_accept(cx) else {
            return Poll::Pending;
        };

        let Ok((socket, addr)) = result else {
            return Poll::Ready(result);
        };

        match socket.set_nodelay(true) {
            Ok(..) => Poll::Ready(Ok((socket, addr))),
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}
