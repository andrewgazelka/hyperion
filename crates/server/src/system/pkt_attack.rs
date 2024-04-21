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
pub struct AttackPlayerQuery<'a> {
    id: EntityId,
    encoder: &'a mut LocalEncoder,
    _player: With<&'static Player>,
}

#[derive(Query)]
pub struct AttackEntityQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
    reaction: &'a mut EntityReaction,
    vitals: &'a mut Vitals,
    immunity: &'a mut ImmuneStatus,
}

#[instrument(skip_all, level = "trace")]
// Check immunity of the entity being attacked
pub fn check_immunity(
    global: Single<&crate::global::Global>,
    attack: ReceiverMut<AttackEntity, &ImmuneStatus>,
) {
    if attack.query.is_invincible(&global) {
        EventMut::take(attack.event);
    }
}

/// send Packet to player encoder
#[instrument(skip_all, level = "trace")]
pub fn pkt_attack_player(
    global: Single<&crate::global::Global>,
    attack: Receiver<AttackEntity, AttackPlayerQuery>,
) {
    let AttackPlayerQuery {
        id: entity_id,
        encoder,
        _player,
    } = attack.query;

    let mut damage_broadcast = get_package(entity_id);
    // local is id 0
    damage_broadcast.entity_id = VarInt(0);
    encoder.append(&damage_broadcast, &global).unwrap();
}

/// Handle Damage and knockback
#[instrument(skip_all, level = "trace")]
pub fn pkt_attack_entity(
    global: Single<&crate::global::Global>,
    attack: Receiver<AttackEntity, AttackEntityQuery>,
    mut broadcast: Single<&mut BroadcastBuf>,
) {
    let AttackEntityQuery {
        id: entity_id,
        pose,
        reaction,
        vitals,
        immunity,
    } = attack.query;

    let damage_broadcast = get_package(entity_id);

    broadcast
        .get_round_robin()
        .append_packet(&damage_broadcast)
        .unwrap();

    let event = attack.event;

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

#[instrument(skip_all, level = "trace")]
fn get_package(id: EntityId) -> play::EntityDamageS2c {
    // todo
    play::EntityDamageS2c {
        entity_id: VarInt(id.index().0 as i32),
        source_type_id: VarInt::default(),
        source_cause_id: VarInt::default(),
        source_direct_id: VarInt::default(),
        source_pos: None,
    }
}
