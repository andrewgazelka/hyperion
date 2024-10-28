use std::io::Write;

use valence_protocol::packets::play;
use valence_text::IntoText;

use crate::PacketBundle;

pub struct Chat {
    raw: play::GameMessageS2c<'static>,
}

pub fn chat(chat: impl Into<String>) -> Chat {
    let chat = chat.into();
    Chat {
        raw: play::GameMessageS2c {
            chat: chat.into_cow_text(),
            overlay: false,
        },
    }
}

impl PacketBundle for &Chat {
    fn encode_including_ids(self, mut w: impl Write) -> anyhow::Result<()> {
        self.raw.encode_including_ids(&mut w)
    }
}
