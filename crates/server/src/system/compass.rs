use evenio::event::Receiver;
use tracing::instrument;
use valence_protocol::packets::play;

use crate::{
    event,
    net::{Compose},
};

#[instrument(skip_all, level = "trace")]
pub fn compass(r: Receiver<event::Compass>, compose: Compose) {
    // TODO:
//    let event = r.event;
//
//    let packets = r.query;
//
//    let pkt = play::PlayerSpawnPositionS2c {
//        position: event.point_to,
//        angle: 0.0,
//    };
//
//    packets.append(&pkt, &compose).unwrap();
}
