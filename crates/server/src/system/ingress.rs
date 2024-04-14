use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::{instrument, warn};

use crate::{
    global::Global,
    packets,
    singleton::{player_id_lookup::PlayerIdLookup, player_uuid_lookup::PlayerUuidLookup},
    system::IngressSender,
    FullEntityPose, Gametick, Player,
};

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
pub fn ingress(
    _: Receiver<Gametick>,
    global: Single<&Global>,
    id_lookup: Single<&PlayerIdLookup>,
    mut fetcher: Fetcher<(EntityId, &mut Player, &mut FullEntityPose)>,
    lookup: Single<&PlayerUuidLookup>,
    mut sender: IngressSender,
) {
    // uuid to entity id map

    // TODO:
    //    let packets: Vec<_> = vec![];
    //
    //    for packet in packets {
    //        let id = packet.user;
    //        let Some(&player_id) = lookup.get(&id) else {
    //            return;
    //        };
    //
    //        let Ok((_, player, position)) = fetcher.get_mut(player_id) else {
    //            return;
    //        };
    //
    //        let packet = packet.packet;
    //
    //        if let Err(e) = packets::switch(
    //            packet,
    //            &global,
    //            player_id,
    //            player,
    //            position,
    //            &id_lookup,
    //            &mut sender,
    //        ) {
    //            let reason = format!("error: {e}");
    //
    //            // todo: handle error
    //            // let _ = player.packets.writer.send_chat_message(&reason);
    //
    //            warn!("invalid packet: {reason}");
    //        }
    //    }
}
