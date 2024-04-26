use evenio::{event::Receiver, fetch::Single};
use valence_protocol::{packets::play, VarInt};

use crate::{
    event,
    event::Scratch,
    net::{Compressor, IoBufs},
};

#[allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]
pub fn block_update(
    r: Receiver<event::UpdateBlock>,
    mut io: Single<&mut IoBufs>,
    mut compressor: Single<&mut Compressor>,
    broadcast: Single<&crate::net::Broadcast>,
) {
    let event = r.event;

    let pkt = play::BlockUpdateS2c {
        position: event.position,
        block_id: event.id,
    };

    let mut scratch = Scratch::new();

    println!("sending block update");

    broadcast
        .append(&pkt, io.one(), &mut scratch, compressor.one())
        .unwrap();

    let pkt = play::PlayerActionResponseS2c {
        sequence: VarInt(event.sequence),
    };

    broadcast
        .append(&pkt, io.one(), &mut scratch, compressor.one())
        .unwrap();
}
