//     private UUID secret;
//     private int serverPort;
//     private UUID playerUUID;
//     private ServerConfig.Codec codec;
//     private int mtuSize;
//     private double voiceChatDistance;
//     private int keepAlive;
//     private boolean groupsEnabled;
//     private String voiceHost;
//     private boolean allowRecording;

use valence_protocol::Encode;

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
