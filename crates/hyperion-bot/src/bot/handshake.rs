use antithesis::random::AntithesisRng;
use rand::Rng;
use tokio::io::AsyncWriteExt;
use valence_protocol::{
    Bounded, Encode, PROTOCOL_VERSION, VarInt, packets,
    packets::handshaking::handshake_c2s::HandshakeNextState,
};

use crate::{bot::Bot, util::random_either};

impl Bot {
    pub async fn handshake(mut self) -> eyre::Result<()> {
        let mut rng = AntithesisRng;

        let protocol_version: i32 = random_either(|| PROTOCOL_VERSION, || rng.r#gen::<i32>());

        let is_correct_protocol_version = protocol_version == PROTOCOL_VERSION;

        let addr = "placeholder";

        let packet = packets::handshaking::HandshakeC2s {
            protocol_version: VarInt(protocol_version),
            server_address: Bounded(addr),
            server_port: 25565, // probably does not matter
            next_state: HandshakeNextState::Status,
        };

        packet
            .encode(&mut self.buf)
            .map_err(|e| eyre::eyre!("failed to encode handshake packet: {e}"))?;

        self.connection.write_all(&self.buf).await?;

        Ok(())
    }
}
