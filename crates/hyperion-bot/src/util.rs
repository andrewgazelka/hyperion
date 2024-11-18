use antithesis::random::AntithesisRng;
use rand::Rng;

pub fn random_either<T>(left: impl FnOnce() -> T, right: impl FnOnce() -> T) -> T {
    let mut rng = AntithesisRng;
    if rng.r#gen::<bool>() { left() } else { right() }
}

use tokio::{
    net::TcpStream,
    time::{Duration, sleep},
};
use tracing::{info, warn};

pub async fn wait_for_tcp_port(addr: &str) {
    info!("Waiting for TCP port to become available at {}", addr);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => {
                info!("Successfully connected to {}", addr);
                break;
            }
            Err(e) => {
                warn!("Failed to connect to {}: {}. Retrying in 1s...", addr, e);
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
