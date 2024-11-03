use antithesis::{assert_sometimes, random::AntithesisRng};
use derive_more::Constructor;
use rand::{rngs::StdRng, Rng};
use uuid::Uuid;
use valence_protocol::{
    packets, packets::handshaking::handshake_c2s::HandshakeNextState, Bounded, VarInt,
    PROTOCOL_VERSION,
};

mod config;
pub use config::Config;

mod generate;
mod util;

mod handshake;

#[derive(Constructor)]
struct Bot {
    name: String,
    uuid: Uuid,
    connection: tokio::net::TcpStream,
}

pub fn bootstrap(config: &Config) {
    // todo: use life cycle

    let mut rng = AntithesisRng;

    let first_name = generate::name();
    assert_sometimes!(first_name.is_valid, "First name is never invalid");
    assert_sometimes!(!first_name.is_valid, "First name is always valid");
    
    let first_uuid: u128 = rng.r#gen();
    let first_uuid = Uuid::from_u128(first_uuid);
    
    let first_handle = tokio::spawn(login(first_bot));
    let second_handle = tokio::spawn(login(second_bot));
}

