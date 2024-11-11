use antithesis::random::AntithesisRng;
use rand::Rng;
use tokio::io::AsyncWriteExt;
use valence_protocol::{
    Bounded, Encode, PROTOCOL_VERSION, VarInt, packets,
    packets::handshaking::handshake_c2s::HandshakeNextState,
};

use crate::{Bot, util::random_either};

impl Bot {
    pub async fn handshake(mut self) -> eyre::Result<()> {
        let mut rng = AntithesisRng;

        let protocol_version: i32 = random_either(|| PROTOCOL_VERSION, || rng.r#gen::<i32>());

        let correct_protocol_version = protocol_version == PROTOCOL_VERSION;
        let protocol_version = VarInt(protocol_version);

        let addr = "placeholder";

        // want to test {correct_protocol_version} <=> pass

        let packet = packets::handshaking::HandshakeC2s {
            protocol_version: VarInt(0),
            server_address: Bounded(addr),
            server_port: 0,
            next_state: HandshakeNextState::Status,
        };

        packet
            .encode(&mut self.buf)
            .map_err(|e| eyre::eyre!("failed to encode handshake packet: {e}"))?;

        self.connection.write_all(&self.buf).await?;

        Ok(())
    }
}
