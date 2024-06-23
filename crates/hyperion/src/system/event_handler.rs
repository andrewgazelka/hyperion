// use evenio::prelude::*;
// use tracing::instrument;
// use valence_protocol::{packets::play, VarInt};
//
// use crate::{
//     components::{EntityReaction, FullEntityPose, ImmuneStatus, Player, Vitals},
//     event::AttackEntity,
//     net::{Compose, StreamId},
// };
//
// #[derive(Query)]
// pub struct AttackPlayerQuery<'a> {
//     id: EntityId,
//     packets: &'a mut StreamId,
//     _player: With<&'static Player>,
// }
//
// #[derive(Query)]
// pub struct AttackEntityQuery<'a> {
//     id: EntityId,
//     pose: &'a FullEntityPose,
//     reaction: &'a mut EntityReaction,
//     immunity: &'a mut ImmuneStatus,
//     vitals: &'a mut Vitals,
// }

use flecs_ecs::{
    core::{
        flecs::pipeline::OnUpdate, Entity, IdOperations, QueryBuilderImpl, SystemAPI,
        TermBuilderImpl, World,
    },
    macros::system,
};
use tracing::instrument;
use valence_protocol::{packets::play, Encode, RawBytes, VarInt};
use valence_text::IntoText;

use crate::{
    component::{Health, InGameName},
    event::{Allocator, AttackEntity, EventQueue, EventQueueIterator, PostureUpdate},
    net::{Compose, NetworkStreamRef},
};

// #[instrument(skip_all, level = "trace")]
// // Check immunity of the entity being attacked
// pub fn check_immunity(
//     global: Single<&crate::global::Global>,
//     attack: ReceiverMut<AttackEntity, &ImmuneStatus>,
// ) {
//     if attack.query.is_invincible(&global) {
//         EventMut::take(attack.event);
//     }
// }

// /// send Packet to player encoder
// #[instrument(skip_all, level = "trace")]
// pub fn send_pkt_attack_player(world: &World) {
//     world
//         .observer_named::<AttackEntity, (&IoRef, &Compose, &flecs::Any)>("send_pkt_attack_player")
//         .term_at(0)
//         .filter()
//         .term_at(1)
//         .singleton()
//         .each_iter(|iter, _, (stream, compose, _)| {
//             let event = iter.param();
//
//             let mut damage_broadcast = get_red_hit_packet(event.from);
//             damage_broadcast.entity_id = VarInt(0);
//
//             compose.unicast(&damage_broadcast, stream).unwrap();
//         });
// }

// pub fn packet_attack_entity(world: &'static World) {
//     world
//         .observer_named::<AttackEntity, (&mut StreamId, &Compose, &flecs::Any)>(
//             "packet_attack_entity",
//         )
//         .term_at(0)
//         .filter()
//         .term_at(1)
//         .filter()
//         .each_iter(|iter, idx, (stream, compose, _)| {
//             let event = iter.param();
//
//             let mut damage_broadcast = get_package(event.from);
//             damage_broadcast.entity_id = VarInt(0);
//
//             compose.unicast(&damage_broadcast, stream).unwrap();
//         });
// }

// /// Handle Damage and knockback
// #[instrument(skip_all, level = "trace")]
// pub fn pkt_attack_entity(
//     global: Single<&crate::global::Global>,
//     attack: Receiver<AttackEntity, AttackEntityQuery>,
//     compose: Compose,
// ) {
//     let AttackEntityQuery {
//         id: entity_id,
//         pose,
//         reaction,
//         vitals,
//         immunity,
//     } = attack.query;
//
//     let damage_broadcast = get_package(entity_id);
//
//     compose.broadcast(&damage_broadcast).send().unwrap();
//
//     let event = attack.event;
//
//     let this = pose.position;
//     let other = event.from_pos;
//
//     let delta_x = other.x - this.x;
//     let delta_z = other.z - this.z;
//
//     if delta_x.abs() < 0.01 && delta_z.abs() < 0.01 {
//         // todo: implement like vanilla
//         return;
//     }
//
//     let dist_xz = delta_x.hypot(delta_z);
//     let multiplier = 0.4;
//
//     reaction.velocity.x /= 2.0;
//     reaction.velocity.y /= 2.0;
//     reaction.velocity.z /= 2.0;
//     reaction.velocity.x -= delta_x / dist_xz * multiplier;
//     reaction.velocity.y += multiplier;
//     reaction.velocity.z -= delta_z / dist_xz * multiplier;
//
//     if reaction.velocity.y > 0.4 {
//         reaction.velocity.y = 0.4;
//     }
//
//     vitals.hurt(&global, event.damage, immunity);
// }

pub fn handle_events(world: &World) {
    system!(
        "handle_events",
        world,
        &Compose($),
        &mut EventQueue,
        &NetworkStreamRef,
        &InGameName,
        &mut Health,
    )
    .multi_threaded()
    .each_entity(|view, (compose, event_queue, stream, ign, health)| {
        let span = tracing::trace_span!("handle_events");
        let _enter = span.enter();
        let mut iter = EventQueueIterator::default();

        let entity_id = view.id();
        let entity_id = VarInt(entity_id.0 as i32);

        // attack
        iter.register::<AttackEntity>(move |_| {
            let damage_broadcast = get_red_hit_packet(view.id());
            compose.broadcast(&damage_broadcast).send().unwrap();

            health.normal -= 1.0;

            // https://wiki.vg/Entity_metadata#Entity_Metadata_Format
            // 9 = Health, type = float
            let mut bytes = Vec::new();
            bytes.push(9_u8);
            VarInt(3).encode(&mut bytes).unwrap();
            health.normal.encode(&mut bytes).unwrap();

            // end with 0xff
            bytes.push(0xff);

            let tracker = play::EntityTrackerUpdateS2c {
                entity_id,
                tracked_values: RawBytes(&bytes),
            };

            compose.broadcast(&tracker).send().unwrap();

            // send chat message
            let msg = format!("{ign} -> health: {health}");

            compose
                .broadcast(&play::GameMessageS2c {
                    chat: msg.into_cow_text(),
                    overlay: false,
                })
                .send()
                .unwrap();
        })
        .unwrap();

        iter.register::<PostureUpdate>(move |posture| {
            // Server to Client (S2C):
            // Entity Metadata packet (0x52).

            // https://wiki.vg/Entity_metadata#Entity_Metadata_Format

            // Index	Unsigned Byte
            // Type	VarInt Enum	 (Only if Index is not 0xff; the type of the index, see the table below)
            // Value	Varies	Only if Index is not 0xff: the value of the metadata field, see the table below

            // for entity index=6 is pose
            // pose had id of 20

            // 6
            // 20
            // varint

            let mut bytes = Vec::new();
            bytes.push(6_u8);
            VarInt(20).encode(&mut bytes).unwrap();

            VarInt(posture.state as i32).encode(&mut bytes).unwrap();

            // end with 0xff
            bytes.push(0xff);

            let tracker = play::EntityTrackerUpdateS2c {
                entity_id,
                tracked_values: RawBytes(&bytes),
            };

            compose.broadcast(&tracker).exclude(stream).send().unwrap();
        })
        .unwrap();

        iter.run(event_queue);
    });
}

pub fn reset_event_queue(world: &World) {
    system!("reset_event_queue", world, &mut EventQueue,)
        .kind::<OnUpdate>()
        .multi_threaded()
        .each(|event_queue| {
            let span = tracing::trace_span!("reset_event_queue");
            let _enter = span.enter();
            event_queue.reset();
        });
}

pub fn reset_allocators(world: &World) {
    system!("reset_allocators", world, &mut Allocator)
        .kind::<OnUpdate>()
        .each(|allocator| {
            let span = tracing::trace_span!("reset_allocators");
            let _enter = span.enter();
            allocator.iter_mut().for_each(|allocator| {
                allocator.reset();
            });
        });
}

#[instrument(skip_all, level = "trace")]
fn get_red_hit_packet(id: Entity) -> play::EntityDamageS2c {
    // todo
    play::EntityDamageS2c {
        entity_id: VarInt(id.0 as i32),
        source_type_id: VarInt::default(),
        source_cause_id: VarInt::default(),
        source_direct_id: VarInt::default(),
        source_pos: None,
    }
}
