use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{packets::play, VarInt};

use crate::{
    net::Encoder, singleton::broadcast::BroadcastBuf, AttackEntity, EntityReaction, FullEntityPose,
    Player,
};

#[instrument(skip_all, level = "trace")]
pub fn pkt_attack(
    global: Single<&crate::global::Global>,
    attack: Receiver<
        AttackEntity,
        (
            EntityId,
            &FullEntityPose,
            &mut EntityReaction,
            &mut Encoder,
            &mut Player,
        ),
    >,
    mut broadcast: Single<&mut BroadcastBuf>,
) {
    let event = attack.event;
    let (entity_id, pose, reaction, encoder, player) = attack.query;

    // todo
    let mut damage_broadcast = play::EntityDamageS2c {
        entity_id: VarInt(entity_id.index().0 as i32),
        source_type_id: VarInt::default(),
        source_cause_id: VarInt::default(),
        source_direct_id: VarInt::default(),
        source_pos: None,
    };

    broadcast
        .get_round_robin()
        .append_packet(&damage_broadcast)
        .unwrap();

    // local is id 0
    damage_broadcast.entity_id = VarInt(0);
    encoder.encode(&damage_broadcast).unwrap();

    let this = pose.position;
    let other = event.from_pos;

    let delta_x = other.x - this.x;
    let delta_z = other.z - this.z;

    if delta_x.abs() < 0.01 && delta_z.abs() < 0.01 {
        // todo: implement like vanilla
        return;
    }

    let dist_xz = delta_x.hypot(delta_z);
    let multiplier = 0.4;

    reaction.velocity.x /= 2.0;
    reaction.velocity.y /= 2.0;
    reaction.velocity.z /= 2.0;
    reaction.velocity.x -= delta_x / dist_xz * multiplier;
    reaction.velocity.y += multiplier;
    reaction.velocity.z -= delta_z / dist_xz * multiplier;

    if reaction.velocity.y > 0.4 {
        reaction.velocity.y = 0.4;
    }

    player.hurt(global.tick.unsigned_abs(), 1.0);
}
