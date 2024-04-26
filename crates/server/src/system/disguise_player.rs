use std::borrow::Cow;

use evenio::{
    entity::EntityId,
    event::{EventMut, ReceiverMut},
    fetch::Single,
    query::Query,
};
use valence_protocol::packets::play;

use crate::{
    components::{FullEntityPose, Uuid},
    event,
    event::Scratch,
    net::{Broadcast, Compressor, IoBufs},
    system::init_entity::spawn_entity_packet,
};

#[derive(Query)]
pub struct DisguisePlayerQuery<'a> {
    id: EntityId,
    uuid: &'a Uuid,
    pose: &'a FullEntityPose,
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]
pub fn disguise_player(
    r: ReceiverMut<event::DisguisePlayer, DisguisePlayerQuery>,
    mut io: Single<&mut IoBufs>,
    mut compressor: Single<&mut Compressor>,
    broadcast: Single<&Broadcast>,
) {
    let event = EventMut::take(r.event);
    let query = r.query;
    let uuids = &[query.uuid.0];

    // remove player
    let pkt = play::PlayerRemoveS2c {
        uuids: Cow::Borrowed(uuids),
    };

    let mut scratch = Scratch::new();

    broadcast
        .append(&pkt, io.one(), &mut scratch, compressor.one())
        .unwrap();

    // spawn entity with same id
    let pkt = spawn_entity_packet(query.id, event.mob, *query.uuid, query.pose);

    broadcast
        .append(&pkt, io.one(), &mut scratch, compressor.one())
        .unwrap();
}
