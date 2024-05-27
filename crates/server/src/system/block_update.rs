use evenio::{event::Receiver, fetch::Single};
use valence_protocol::{packets::play, VarInt};

use crate::{event, net::Compose};

#[allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]
pub fn block_update(
    r: Receiver<event::UpdateBlock>,
    broadcast: Single<&crate::net::Broadcast>,
    encode: Compose,
) {
    let event = r.event;

    let pkt = play::BlockUpdateS2c {
        position: event.position,
        block_id: event.id,
    };

    broadcast.append(&pkt, &encode).unwrap();

    // todo: I feel like the response should go before, no?
    let pkt = play::PlayerActionResponseS2c {
        sequence: VarInt(event.sequence),
    };

    broadcast.append(&pkt, &encode).unwrap();
}
