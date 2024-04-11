use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{packets::play, Hand, VarInt};

use crate::{singleton::broadcast::BroadcastBuf, SwingArm};

#[instrument(skip_all, level = "trace")]
pub fn pkt_hand_swing(
    swing_arm: Receiver<SwingArm, EntityId>,
    mut broadcast: Single<&mut BroadcastBuf>,
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

    broadcast.get_round_robin().append_packet(&pkt).unwrap();
}
