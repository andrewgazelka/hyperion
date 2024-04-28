use std::borrow::Cow;

use evenio::{
    entity::EntityId,
    event::{EventMut, ReceiverMut},
    fetch::Fetcher,
    query::Query,
};
use valence_protocol::packets::play;

use crate::{
    components::{FullEntityPose, Uuid},
    event,
    net::{Compose, Packets},
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
    all_packets: Fetcher<(&Packets, EntityId)>,
    compose: Compose,
) {
    let event = EventMut::take(r.event);
    let query = r.query;
    let uuids = &[query.uuid.0];

    // todo: add broadcast with mask
    for (packets, id) in all_packets {
        if id == query.id {
            continue;
        }

        // remove player
        let pkt = play::PlayerRemoveS2c {
            uuids: Cow::Borrowed(uuids),
        };

        packets.append(&pkt, &compose).unwrap();

        // spawn entity with same id
        let pkt = spawn_entity_packet(query.id, event.mob, *query.uuid, query.pose);

        packets.append(&pkt, &compose).unwrap();
    }
}
