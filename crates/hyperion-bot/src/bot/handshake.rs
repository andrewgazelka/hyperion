use antithesis::{assert_always, assert_never, random::AntithesisRng};
use eyre::bail;
use rand::Rng;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use valence_protocol::{
    Bounded, PROTOCOL_VERSION, Packet, VarInt, packets,
    packets::handshaking::handshake_c2s::HandshakeNextState,
};

use crate::{bot::Bot, util::random_either};

impl Bot {
    pub async fn handshake(mut self) -> eyre::Result<()> {
        let mut rng = AntithesisRng;

        let protocol_version: i32 = random_either(|| PROTOCOL_VERSION, || rng.r#gen::<i32>());

        let addr = "placeholder";

        let packet = packets::handshaking::HandshakeC2s {
            protocol_version: VarInt(protocol_version),
            server_address: Bounded(addr),
            server_port: 25565, // probably does not matter
            next_state: HandshakeNextState::Status,
        };

        let result = self.encoder.append_packet(&packet);

        assert_always!(result.is_ok(), "Failed to encode handshake packet");

        let packet = packets::status::QueryRequestC2s;

        let result = self.encoder.append_packet(&packet);

        assert_always!(result.is_ok(), "Failed to encode handshake packet");

        let bytes = self.encoder.take();

        self.connection.write_all(&bytes).await?;

        let result_packet = self.connection.read_buf(&mut self.decode_buf).await?;

        assert_never!(result_packet == 0, "Failed to read handshake packet");

        println!("read bytes {:?}", self.decode_buf);
        self.decoder.queue_bytes(self.decode_buf.split());

        let packet1 = self.decoder.try_next_packet();

        let Ok(packet1) = packet1 else {
            antithesis::assert_unreachable!("Failed to decode handshake packet");
            bail!("Failed to decode handshake packet");
        };

        let Some(packet1) = packet1 else {
            antithesis::assert_unreachable!("Failed to decode handshake packet");
            bail!("Failed to decode handshake packet");
        };

        assert_always!(
            packet1.id == packets::status::QueryResponseS2c::ID,
            "Failed to decode handshake packet"
        );

        let packet: packets::status::QueryResponseS2c<'_> = packet1.decode().unwrap();

        let json = packet.json;

        // todo: maybe remove unwrap and use antithesis asserts first
        let json: serde_json::Value = serde_json::from_str(json).unwrap();

        let description = json.get("description").unwrap();

        let description = description.as_str().unwrap();

        assert_always!(
            description
                == "Getting 10k Players to PvP at Once on a Minecraft Server to Break the \
                    Guinness World Record",
            "Failed to decode handshake packet"
        );

        Ok(())
    }
}
