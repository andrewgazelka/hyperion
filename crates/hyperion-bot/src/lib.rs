use antithesis::{assert_sometimes, random::AntithesisRng};
use rand::Rng;
use tokio::task::JoinSet;
use uuid::Uuid;

mod config;
pub use config::Config;

use crate::bot::Bot;

mod generate;
mod util;

mod bot;

pub async fn bootstrap(config: &Config) {
// Wait for TCP port to be available
    util::wait_for_tcp_port(&config.host).await;
    // todo: use life cycle

    let mut rng = AntithesisRng;
    
    let first_name = generate::name();
    assert_sometimes!(first_name.is_valid, "First name is never invalid");
    assert_sometimes!(!first_name.is_valid, "First name is always valid");

    let first_uuid: u128 = rng.r#gen();
    let first_uuid = Uuid::from_u128(first_uuid);
    
    let mut join_set = JoinSet::new();

    for _ in 0..config.max_number_of_bots {
        let name = first_name.value.clone();
        let addr = config.host.clone();
        join_set.spawn(async move {
            let bot = Bot::new(name, first_uuid, addr);
            bot.await.handshake().await.unwrap();
        });
    }
    
    join_set.join_all().await;
}
