use evenio::event::Receiver;
use tracing::instrument;
use valence_protocol::packets::play;

use crate::{
    event,
    net::{Compose, StreamId},
};

#[instrument(skip_all, level = "trace")]
pub fn compass(r: Receiver<event::PointCompass, &mut StreamId>, compose: Compose) {
    let event = r.event;

    let packets = r.query;

    let pkt = play::PlayerSpawnPositionS2c {
        position: event.point_to,
        angle: 0.0,
    };

    compose.unicast(&pkt, packets).unwrap();
}
