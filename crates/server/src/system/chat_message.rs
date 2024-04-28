use evenio::event::{EventMut, ReceiverMut};
use valence_protocol::packets::play;

use crate::{
    event,
    net::{Compose, Packets},
};

#[allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]
pub fn chat_message(r: ReceiverMut<event::ChatMessage, &mut Packets>, compose: Compose) {
    let event = EventMut::take(r.event);
    let packets = r.query;

    let pkt = play::GameMessageS2c {
        chat: event.message.into(),
        overlay: false,
    };

    packets.append(&pkt, &compose).unwrap();
}
