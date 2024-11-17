use antithesis::{assert_sometimes, random::AntithesisRng};
use rand::Rng;
use uuid::Uuid;

mod config;
pub use config::Config;

use crate::bot::Bot;

mod generate;
mod util;

mod bot;
mod handshake;

pub fn bootstrap(config: &Config) {
    // todo: use life cycle

    let mut rng = AntithesisRng;
    
    let first_name = generate::name();
    assert_sometimes!(first_name.is_valid, "First name is never invalid");
    assert_sometimes!(!first_name.is_valid, "First name is always valid");

    let first_uuid: u128 = rng.r#gen();
    let first_uuid = Uuid::from_u128(first_uuid);

    for _ in 0..10 {
        let name = first_name.value.clone();
        let addr = config.host.clone();
        tokio::spawn(async move {
            let bot = Bot::new(name, first_uuid, addr);
            bot.await.handshake().await.unwrap();
        });
    }
}
