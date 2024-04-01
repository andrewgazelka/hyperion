use std::cell::Cell;

use bytes::Bytes;
use evenio::{event::Receiver, fetch::Fetcher, query::Not};
use fastrand::Rng;
use tracing::{instrument, trace};
use valence_protocol::math::DVec2;

use crate::{
    singleton::encoder::Encoder, BroadcastPackets, FullEntityPose, MinecraftEntity, Player, Uuid,
};

#[thread_local]
static RNG: Cell<Option<Rng>> = Cell::new(None);

// TODO: Split broadcast_packets into separate functions
#[allow(clippy::cognitive_complexity)]
#[instrument(skip_all)]
pub fn broadcast_packets(
    _: Receiver<BroadcastPackets>,
    player: Fetcher<(&Uuid, &FullEntityPose, &Player, Not<&MinecraftEntity>)>,
) {
    let start = std::time::Instant::now();

    Encoder::par_drain(|encoder| {
        if encoder.necessary_packets.is_empty() && encoder.droppable_packets.is_empty() {
            return;
        }

        let start = std::time::Instant::now();

        let mut rng = RNG.take().unwrap_or_default();

        // TODO: Avoid taking packet_data so that the capacity can be reused
        let packet_data = Bytes::from(core::mem::take(&mut encoder.packet_data));

        for (player_uuid, pose, player, _) in &player {
            let player_location = DVec2::new(pose.position.x, pose.position.y);

            // Max bytes that should be sent this tick
            // TODO: Determine max_bytes using the player's network speed, latency, and current
            // send window size
            let max_bytes = 25_000; // 4 Mbit/s
            let mut total_bytes_sent = 0;

            for packet in &encoder.necessary_packets {
                if packet.exclude_player == Some(player_uuid.0) {
                    continue;
                }

                if player
                    .packets
                    .writer
                    .send_raw(packet_data.slice(packet.offset..packet.offset + packet.len))
                    .is_err()
                {
                    return;
                }
                total_bytes_sent += packet.len;
            }

            if total_bytes_sent < max_bytes {
                let all_droppable_packets_len = encoder
                    .droppable_packets
                    .iter()
                    .map(|packet| packet.len)
                    .sum::<usize>();
                if all_droppable_packets_len + total_bytes_sent <= max_bytes {
                    for packet in &encoder.droppable_packets {
                        if packet.exclude_player == Some(player_uuid.0) {
                            continue;
                        }

                        if player
                            .packets
                            .writer
                            .send_raw(packet_data.slice(packet.offset..packet.offset + packet.len))
                            .is_err()
                        {
                            return;
                        }

                        // total_bytes_sent is not increased because it is no longer used
                    }
                } else {
                    // todo: remove shuffling; this is inefficient
                    rng.shuffle(&mut encoder.droppable_packets);
                    for packet in &encoder.droppable_packets {
                        if packet.exclude_player == Some(player_uuid.0) {
                            continue;
                        }

                        // TODO: Determine chance better
                        // This currently picks packets closest to the front more often than
                        // packets in the back. To compensate for this, droppable_packets is
                        // shuffled, but ideally shuffling shouldn't be necessary.
                        let distance_squared =
                            packet.prioritize_location.distance_squared(player_location);
                        let chance = (1.0 / distance_squared).clamp(0.05, 1.0);
                        let chance_u8 = (chance * 255.0) as u8;
                        let keep = rng.u8(..) > chance_u8;

                        if !keep {
                            continue;
                        }

                        if total_bytes_sent + packet.len > max_bytes {
                            // In theory, this loop could keep going if the current packet is large
                            // and the rest of the packets are small. However, most of these
                            // droppable packets are small, so it's not worth it to check the rest
                            // of the packets.
                            break;
                        }

                        if player
                            .packets
                            .writer
                            .send_raw(packet_data.slice(packet.offset..packet.offset + packet.len))
                            .is_err()
                        {
                            return;
                        }

                        total_bytes_sent += packet.len;
                    }
                }
            }
        }

        RNG.set(Some(rng));
        encoder.clear_packets();

        trace!(
            "took {:?} to broadcast packets with specific encoder",
            start.elapsed()
        );
    });
    trace!("took {:?} to broadcast packets", start.elapsed());
}
