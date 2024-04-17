use evenio::prelude::*;
use tracing::instrument;
use valence_protocol::{packets::play, VarInt};

use crate::{
    components::{EntityReaction, FullEntityPose, ImmuneStatus, Player, Vitals},
    events::AttackEntity,
    net::LocalEncoder,
    singleton::broadcast::BroadcastBuf,
};

#[derive(Query)]
pub struct AttackQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
    reaction: &'a mut EntityReaction,
    encoder: &'a mut LocalEncoder,
    immunity: &'a mut ImmuneStatus,
    vitals: &'a mut Vitals,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn pkt_attack(
    global: Single<&crate::global::Global>,
    attack: Receiver<AttackEntity, AttackQuery>,
    mut broadcast: Single<&mut BroadcastBuf>,
) {
    let AttackQuery {
        id: entity_id,
        pose,
        reaction,
        encoder,
        immunity,
        vitals,
        _player,
    } = attack.query;

    if immunity.is_invincible(&global) {
        return;
    }

    let event = attack.event;

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
    encoder.append(&damage_broadcast, &global).unwrap();

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

    vitals.hurt(&global, 1.0, immunity);
}
