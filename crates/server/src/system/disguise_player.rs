use std::borrow::Cow;

use evenio::{
    entity::EntityId,
    event::{EventMut, Insert, ReceiverMut, Sender},
    fetch::Fetcher,
    query::Query,
};
use tracing::instrument;
use valence_protocol::packets::play;

use crate::{
    components::{Display, FullEntityPose, Uuid},
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

#[instrument(skip_all, level = "trace")]
pub fn disguise_player(
    r: ReceiverMut<event::DisguisePlayer, DisguisePlayerQuery>,
    all_packets: Fetcher<(&mut Packets, EntityId)>,
    compose: Compose,
    sender: Sender<Insert<Display>>,
) {
    let event = EventMut::take(r.event);
    let query = r.query;
    let uuids = &[query.uuid.0];

    let remove_pkt = play::PlayerRemoveS2c {
        uuids: Cow::Borrowed(uuids),
    };
    let spawn_pkt = spawn_entity_packet(query.id, event.mob, *query.uuid, query.pose);

    let mut bytes = Vec::new();

    compose
        .encoder()
        .append_packet(
            &remove_pkt,
            &mut bytes,
            &mut *compose.scratch().borrow_mut(),
            &mut compose.compressor().borrow_mut(),
        )
        .unwrap();

    compose
        .encoder()
        .append_packet(
            &spawn_pkt,
            &mut bytes,
            &mut *compose.scratch().borrow_mut(),
            &mut compose.compressor().borrow_mut(),
        )
        .unwrap();

    // todo: add broadcast with mask
    for (packets, id) in all_packets {
        if id == query.id {
            continue;
        }

        compose.io_buf().unicast_raw(&bytes, packets.id());
    }

    sender.insert(query.id, Display(event.mob));
}
