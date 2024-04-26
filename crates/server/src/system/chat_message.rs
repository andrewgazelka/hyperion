use evenio::{
    event::{EventMut, Receiver, ReceiverMut},
    fetch::Single,
};
use valence_protocol::{packets::play, VarInt};

use crate::{
    event,
    event::Scratch,
    net::{Compressor, IoBufs, Packets},
};

#[allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]
pub fn chat_message(
    r: ReceiverMut<event::ChatMessage, &mut Packets>,
    mut io: Single<&mut IoBufs>,
    mut compressor: Single<&mut Compressor>,
) {
    let event = EventMut::take(r.event);
    let packets = r.query;

    let pkt = play::GameMessageS2c {
        chat: event.message.into(),
        overlay: false,
    };

    let mut scratch = Scratch::new();

    packets
        .append(&pkt, io.one(), &mut scratch, compressor.one())
        .unwrap();
}
