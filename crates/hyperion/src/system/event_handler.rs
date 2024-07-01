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
        flecs::pipeline, Entity, IntoWorld, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World,
    },
    macros::system,
};
use glam::IVec3;
use rayon::iter::ParallelIterator;
use tracing::{info, info_span, instrument, trace_span};
use valence_generated::block::BlockState;
use valence_protocol::{
    ident,
    packets::play,
    sound::{SoundCategory, SoundId},
    VarInt,
};

use crate::{
    component::{
        blocks::{
            loaded::{Delta, LoadedChunk, NeighborNotify, PendingChanges},
            MinecraftWorld,
        },
        ConfirmBlockSequences,
    },
    event::{BlockBreak, EventQueue, EventQueueIterator, ThreadLocalBump},
    net::{Compose, NetworkStreamRef},
    tracing_ext::TracingExt,
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
pub fn handle_events(world: &World) {
    // let mut chunk_iter = EventQueueIterator::default();
    //
    // chunk_iter.register::<BlockBreak>(|block, query| {
    //     info!("block break: {block:?}");
    // });

    // iterator here

    // let mut iter: EventQueueIterator<Data> = EventQueueIterator::default();
    //
    // // attack
    // iter.register::<AttackEntity>(|_, query| {
    //     let view = query.view;
    //     let compose = query.compose;
    //     let health = &mut *query.health;
    //     let entity_id = query.entity_id;
    //
    //     let damage_broadcast = get_red_hit_packet(view.id());
    //     compose
    //         .broadcast(&damage_broadcast)
    //         .send(query.world)
    //         .unwrap();
    //
    //     health.normal -= 1.0;
    //
    //     // https://wiki.vg/Entity_metadata#Entity_Metadata_Format
    //     // 9 = Health, type = float
    //     let mut bytes = Vec::new();
    //     bytes.push(9_u8);
    //     VarInt(3).encode(&mut bytes).unwrap();
    //     health.normal.encode(&mut bytes).unwrap();
    //
    //     // end with 0xff
    //     bytes.push(0xff);
    //
    //     let tracker = play::EntityTrackerUpdateS2c {
    //         entity_id,
    //         tracked_values: RawBytes(&bytes),
    //     };
    //
    //     compose.broadcast(&tracker).send(query.world).unwrap();
    //
    //     let ign = query.ign;
    //
    //     // send chat message
    //     let msg = format!("{ign} -> health: {health}");
    //
    //     compose
    //         .broadcast(&play::GameMessageS2c {
    //             chat: msg.into_cow_text(),
    //             overlay: false,
    //         })
    //         .send(query.world)
    //         .unwrap();
    // });
    //
    // iter.register::<PostureUpdate>(|posture, query| {
    //     // Server to Client (S2C):
    //     // Entity Metadata packet (0x52).
    //
    //     // https://wiki.vg/Entity_metadata#Entity_Metadata_Format
    //
    //     // Index	Unsigned Byte
    //     // Type	VarInt Enum	 (Only if Index is not 0xff; the type of the index, see the table below)
    //     // Value	Varies	Only if Index is not 0xff: the value of the metadata field, see the table below
    //
    //     // for entity index=6 is pose
    //     // pose had id of 20
    //
    //     // 6
    //     // 20
    //     // varint
    //
    //     let mut bytes = Vec::new();
    //     bytes.push(6_u8);
    //     VarInt(20).encode(&mut bytes).unwrap();
    //
    //     VarInt(posture.state as i32).encode(&mut bytes).unwrap();
    //
    //     // end with 0xff
    //     bytes.push(0xff);
    //
    //     let entity_id = query.entity_id;
    //
    //     let tracker = play::EntityTrackerUpdateS2c {
    //         entity_id,
    //         tracked_values: RawBytes(&bytes),
    //     };
    //
    //     let compose = query.compose;
    //     let stream = query.stream;
    //
    //     compose
    //         .broadcast(&tracker)
    //         .exclude(stream)
    //         .send(query.world)
    //         .unwrap();
    // });
    //
    // iter.register::<SwingArm>(|event, query| {
    //     use valence_protocol::Hand;
    //
    //     // Server to Client (S2C):
    //
    //     let hand = event.hand;
    //
    //     let animation = match hand {
    //         Hand::Main => 0,
    //         Hand::Off => 3,
    //     };
    //
    //     let entity_id = query.entity_id;
    //
    //     let pkt = play::EntityAnimationS2c {
    //         entity_id,
    //         animation,
    //     };
    //
    //     let compose = query.compose;
    //
    //     compose.broadcast(&pkt).send(query.world).unwrap();
    // });

    system!(
        "handle_events_block",
        world,
        &Compose($),
        &mut EventQueue,
        &PendingChanges,
    )
    .kind::<pipeline::PostUpdate>()
    .multi_threaded()
    .tracing_each_entity(
        info_span!("handle_events_block"),
        move |entity, (compose, event_queue, pending)| {
            let world = entity.world();
            let world = &world;

            let len = event_queue.len();
            let count = event_queue.count.load(std::sync::atomic::Ordering::Relaxed);
            assert_eq!(len, count);

            // let state = chunk.chunk.block_state(u32::from(position.x), u32::from(position.y), u32::from(position.z));

            // let value = state.get(PropName::Facing);

            let mut iter = EventQueueIterator::default();

            iter.register::<BlockBreak>(|block| {
                assert_ne!(len, 0, "event queue is empty");

                let position = block.position;

                let delta = Delta::new(
                    position.x as u8,
                    position.y,
                    position.z as u8,
                    BlockState::AIR,
                );

                pending.push(delta, world);

                info!("block break: {block:?}");

                let entity = world.entity_from_id(block.by);

                // get NetRef
                entity.get::<&NetworkStreamRef>(|stream| {
                    // confirm
                    // compose
                    //     .unicast(
                    //         &play::PlayerActionResponseS2c { sequence: block.id },
                    //         stream,
                    //         world,
                    //     )
                    //     .unwrap();

                    // compose
                    //     .unicast(
                    //         &play::BlockUpdateS2c {
                    //             position: block.position,
                    //             block_id: BlockState::CYAN_CARPET,
                    //         },
                    //         stream,
                    //         world,
                    //     )
                    //     .unwrap();

                    let id = ident!("minecraft:block.note_block.harp");

                    let id = SoundId::Direct {
                        id: id.into(),
                        range: None,
                    };

                    let sound = play::PlaySoundS2c {
                        id,
                        category: SoundCategory::Ambient,
                        position: IVec3::new(-3720, -120, -352),
                        volume: 1.0,
                        pitch: 1.0,
                        seed: 0,
                    };

                    compose.unicast(&sound, stream, world).unwrap();
                });
            });

            iter.run(event_queue);
        },
    );

    system!(
        "handle_chunk_neighbor_notify",
        world,
        &LoadedChunk,
        &MinecraftWorld($),
        &mut NeighborNotify,
    )
    .kind::<pipeline::PostUpdate>()
    .multi_threaded()
    .tracing_each_entity(
        info_span!("handle_chunk_neighbor_notify"),
        |entity, (chunk, mc, notify)| {
            let world = entity.world();
            chunk.process_neighbor_changes(notify, mc, &world);
        },
    );

    system!(
        "handle_chunk_pending_changes",
        world,
        &mut LoadedChunk,
        &Compose($),
        &MinecraftWorld($),
        &mut PendingChanges,
        &NeighborNotify,
    )
    .kind::<pipeline::PostUpdate>()
    .multi_threaded()
    .tracing_each_entity(
        info_span!("handle_chunk_pending_changes"),
        |entity, (chunk, compose, mc, pending, notify)| {
            let world = entity.world();
            chunk.process_pending_changes(pending, compose, notify, mc, &world);
        },
    );

    system!(
        "confirm_block_sequences",
        world,
        &Compose($),
        &NetworkStreamRef,
        &mut ConfirmBlockSequences,
    )
    .kind::<pipeline::PostUpdate>()
    .multi_threaded()
    .tracing_each_entity(
        info_span!("confirm_block_sequences"),
        |entity, (compose, stream, confirm_block_sequences)| {
            let world = entity.world();
            for sequence in confirm_block_sequences.drain(..) {
                let sequence = VarInt(sequence);
                let ack = play::PlayerActionResponseS2c { sequence };
                compose.unicast(&ack, stream, &world).unwrap();
            }
        },
    );

    // system!(
    //     "handle_events_player",
    //     world,
    //     &Compose($),
    //     &mut EventQueue,
    //     &NetworkStreamRef,
    //     &InGameName,
    //     &mut Health,
    // )
    // .multi_threaded()
    // .each_entity(move |view, (compose, event_queue, stream, ign, health)| {
    //     let span = tracing::info_span!("handle_events");
    //     let _enter = span.enter();
    //
    //     let world = &view.world();
    //
    //     let mut data = Data {
    //         view: &view,
    //         entity_id: VarInt(view.id().0 as i32),
    //         compose,
    //         stream,
    //         ign,
    //         health,
    //         world,
    //     };
    //
    //     iter.run(event_queue, &mut data);
    // });
}

pub fn reset_event_queue(world: &World) {
    system!("reset_event_queue", world, &mut EventQueue)
        .kind::<pipeline::PostUpdate>()
        .multi_threaded()
        .tracing_each(trace_span!("reset_event_queue"), |event_queue| {
            event_queue.reset();
        });
}

pub fn reset_allocators(world: &World) {
    system!(
        "reset_allocators",
        world,
        &mut ThreadLocalBump($)
    )
    .kind::<pipeline::PostUpdate>()
    .each(|allocator| {
        let span = tracing::info_span!("reset_allocators");
        let _enter = span.enter();
        allocator.par_iter_mut().for_each(|allocator| {
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
