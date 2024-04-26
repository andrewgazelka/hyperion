use evenio::{
    event::{EventMut, ReceiverMut},
    fetch::Single,
};
use evenio::entity::EntityId;
use valence_protocol::packets::play;

use crate::{
    event,
    event::Scratch,
    net::{Compressor, IoBufs, Packets},
};
use crate::net::Broadcast;
use crate::singleton::player_id_lookup::EntityIdLookup;

#[allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]
pub fn disguise_player(
    r: ReceiverMut<event::DisguisePlayer, EntityId>,
    mut io: Single<&mut IoBufs>,
    mut compressor: Single<&mut Compressor>,
    id_lookup: Single<&EntityIdLookup>,
    broadcast: Single<&Broadcast>,
) {
    let event = EventMut::take(r.event);
    let packets = r.query;

    packets
        .append(&pkt, io.one(), &mut scratch, compressor.one())
        .unwrap();
}
