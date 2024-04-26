use valence_protocol::{packets::play::CustomPayloadS2c, Bounded, Encode};
use valence_server::Ident;

pub trait Msg: Encode {
    const KEY: Ident<&'static str>;

    fn to_plugin_message(&self) -> CustomPayloadS2c<'static> {
        CustomPayloadS2c {
            channel: Self::KEY.into(),
            data: Bounded::default(),
        }
    }
}

#[derive(Encode)]
pub struct SecretVoiceChatS2c<'a> {
    pub secret: uuid::Uuid,
    pub server_port: i32,
    pub player_uuid: uuid::Uuid,
    pub codec: Codec,
    pub mtu_size: i32,
    pub voice_chat_distance: f64,
    pub keep_alive: i32,
    pub groups_enabled: bool,
    pub voice_host: &'a str,
    pub allow_recording: bool,
}

impl<'a> Msg for SecretVoiceChatS2c<'a> {
    const KEY: Ident<&'static str> = Ident::new_unchecked("voicechat:secret");
}

#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Codec {
    VoIp,
    Audio,
    RestrictedLowdelay,
}

impl Encode for Codec {
    fn encode(&self, mut w: impl std::io::Write) -> anyhow::Result<()> {
        let byte = *self as u8;
        w.write_all(&[byte])?;
        Ok(())
    }
}
