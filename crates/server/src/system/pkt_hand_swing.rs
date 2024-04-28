use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{packets::play, Hand, VarInt};

use crate::{
    event::SwingArm,
    net::{Broadcast, Compose},
};

#[instrument(skip_all, level = "trace")]
pub fn pkt_hand_swing(
    swing_arm: Receiver<SwingArm, EntityId>,
    broadcast: Single<&mut Broadcast>,
    compose: Compose,
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

    broadcast.append(&pkt, &compose).unwrap();
}
