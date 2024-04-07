use bytes::Bytes;
use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
    query::Not,
};
use tracing::{instrument, trace};

use crate::{
    singleton::encoder::Encoder, BroadcastPackets, FullEntityPose, MinecraftEntity, Player, Uuid,
};

#[instrument(skip_all, level = "trace")]
pub fn broadcast_packets(
    _: Receiver<BroadcastPackets>,
    player: Fetcher<(&Uuid, &FullEntityPose, &Player, Not<&MinecraftEntity>)>,
    encoder: Single<&mut Encoder>,
) {
    let encoder = encoder.0;

    encoder.par_drain(|buf| {
        // TODO: Avoid taking packet_data so that the capacity can be reused
        let packet_data = Bytes::from(core::mem::take(&mut buf.packet_data));

        for (_, _, player, _) in &player {
            trace!("about to broadcast bytes {:?}", packet_data.len());
            let _ = player.packets.writer.send_raw(packet_data.clone());
        }

        // RNG.set(Some(rng));
        buf.clear_packets();
    });
}
