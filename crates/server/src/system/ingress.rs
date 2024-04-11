use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
};
use rayon::prelude::*;
use tracing::{instrument, warn};
use valence_protocol::{ByteAngle, VarInt};

use crate::{
    net::GLOBAL_C2S_PACKETS,
    packets,
    singleton::{
        broadcast::{Broadcast, PacketMetadata},
        player_lookup::PlayerUuidLookup,
    },
    system::IngressSender,
    FullEntityPose, Gametick, Player,
};

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
pub fn ingress(
    _: Receiver<Gametick>,
    mut fetcher: Fetcher<(EntityId, &mut Player, &mut FullEntityPose)>,
    lookup: Single<&PlayerUuidLookup>,
    mut sender: IngressSender,
    broadcast: Single<&Broadcast>,
) {
    // uuid to entity id map

    let packets: Vec<_> = core::mem::take(&mut *GLOBAL_C2S_PACKETS.lock());

    for packet in packets {
        let id = packet.user;
        let Some(&player_id) = lookup.get(&id) else {
            return;
        };

        let Ok((_, player, position)) = fetcher.get_mut(player_id) else {
            return;
        };

        let packet = packet.packet;

        if let Err(e) = packets::switch(packet, player_id, player, position, &mut sender) {
            let reason = format!("error: {e}");

            // todo: handle error
            // let _ = player.packets.writer.send_chat_message(&reason);

            warn!("invalid packet: {reason}");
        }
    }

    fetcher.par_iter_mut().for_each(|(id, _, pose)| {
        let pos = pose.position.as_dvec3();

        let packet = valence_protocol::packets::play::EntityPositionS2c {
            entity_id: VarInt(id.index().0 as i32),
            position: pos,
            yaw: ByteAngle::from_degrees(pose.yaw),
            pitch: ByteAngle::from_degrees(pose.pitch),
            // yaw: ByteAngle::from_degrees(pose.yaw),
            // pitch: ByteAngle::from_degrees(pose.pitch),
            on_ground: false,
        };

        // todo: what it panics otherwise
        broadcast
            .append(&packet, PacketMetadata::DROPPABLE)
            .unwrap();

        // look
        let packet = valence_protocol::packets::play::EntitySetHeadYawS2c {
            entity_id: VarInt(id.index().0 as i32),
            head_yaw: ByteAngle::from_degrees(pose.yaw),
        };

        broadcast
            .append(&packet, PacketMetadata::DROPPABLE)
            .unwrap();
    });
}
