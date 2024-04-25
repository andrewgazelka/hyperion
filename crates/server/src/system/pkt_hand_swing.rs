use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{packets::play, Hand, VarInt};

use crate::{
    events::{Scratch, SwingArm},
    net::{Broadcast, Compressor, IoBufs},
};

#[instrument(skip_all, level = "trace")]
pub fn pkt_hand_swing(
    swing_arm: Receiver<SwingArm, EntityId>,
    broadcast: Single<&mut Broadcast>,
    mut io: Single<&mut IoBufs>,
    mut compressor: Single<&mut Compressor>,
) {
    let entity_id = swing_arm.query;
    let entity_id = VarInt(entity_id.index().0 as i32);
    let hand = swing_arm.event.hand;

    let animation = match hand {
        Hand::Main => 0,
        Hand::Off => 3,
    };

    let pkt = play::EntityAnimationS2c {
        entity_id,
        animation,
    };

    let mut scratch = Scratch::new();
    broadcast
        .append(&pkt, io.one(), &mut scratch, compressor.one())
        .unwrap();
}
