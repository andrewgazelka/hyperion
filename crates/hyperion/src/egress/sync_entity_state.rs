use std::borrow::Cow;

use anyhow::Context;
use flecs_ecs::prelude::*;
use glam::Vec3;
use hyperion_inventory::PlayerInventory;
use hyperion_utils::EntityExt;
use tracing::{error, info_span};
use valence_protocol::{
    ByteAngle, RawBytes, VarInt, Velocity,
    packets::{play, play::entity_equipment_update_s2c::EquipmentEntry},
};

use crate::{
    Prev,
    net::{Compose, NetworkStreamRef},
    simulation::{
        EntityReaction, Pitch, Position, Xp, Yaw, animation::ActiveAnimation,
        metadata::MetadataChanges,
    },
    system_registry::{SYNC_ENTITY_POSITION, SystemId},
    util::TracingExt,
};

#[derive(Component)]
pub struct EntityStateSyncModule;

impl Module for EntityStateSyncModule {
    fn module(world: &World) {
        let system_id = SYNC_ENTITY_POSITION;

        world
            .system_named::<(
                &Compose,          // (0)
                &NetworkStreamRef, // (1)
                &mut (Prev, Xp),   // (2)
                &mut Xp,           // (3)
            )>("entity_xp_sync")
            .term_at(0u32)
            .singleton()
            .multi_threaded()
            .kind::<flecs::pipeline::OnStore>()
            .run(|mut table| {
                while table.next() {
                    let count = table.count();
                    let world = table.world();

                    unsafe {
                        const _: () = assert!(size_of::<Xp>() == size_of::<u16>());
                        const _: () = assert!(align_of::<Xp>() == align_of::<u16>());

                        /// Number of lanes in the SIMD vector
                        const LANES: usize = 64; // up to AVX512

                        let compose = table.field_unchecked::<Compose>(0);
                        let compose = compose.first().unwrap();

                        let net = table.field_unchecked::<NetworkStreamRef>(1);
                        let net = net.get(..).unwrap();

                        let mut prev_xp = table.field_unchecked::<Xp>(2);
                        let prev_xp = prev_xp.get_mut(..).unwrap();
                        let prev_xp: &mut [u16] =
                            core::slice::from_raw_parts_mut(prev_xp.as_mut_ptr().cast(), count);

                        debug_assert_eq!(
                            prev_xp.as_ptr() as usize & 63,
                            0,
                            "prev_xp is not 64-byte aligned"
                        );

                        let mut xp = table.field_unchecked::<Xp>(3);
                        let xp = xp.get_mut(..).unwrap();
                        let xp: &mut [u16] =
                            core::slice::from_raw_parts_mut(xp.as_mut_ptr().cast(), count);

                        simd_utils::copy_and_get_diff::<_, LANES>(
                            prev_xp,
                            xp,
                            |idx, prev, current| {
                                debug_assert!(prev != current);

                                let net = net.get(idx).unwrap();

                                let current = Xp::from(*current);
                                let visual = current.get_visual();

                                let packet = play::ExperienceBarUpdateS2c {
                                    bar: visual.prop,
                                    level: VarInt(i32::from(visual.level)),
                                    total_xp: VarInt::default(),
                                };

                                compose
                                    .unicast(&packet, *net, SystemId(100), &world)
                                    .unwrap();
                            },
                        );
                    }
                }
            });

        system!("entity_metadata_sync", world, &Compose($), &mut MetadataChanges)
            .multi_threaded()
            .kind::<flecs::pipeline::OnStore>()
            .tracing_each_entity(
                info_span!("entity_metadata_sync"),
                move |entity, (compose, metadata_changes)| {
                    let world = entity.world();
                    let entity_id = VarInt(entity.minecraft_id());

                    let metadata = metadata_changes.get_and_clear();

                    if let Some(view) = metadata {
                        let pkt = play::EntityTrackerUpdateS2c {
                            entity_id,
                            tracked_values: RawBytes(&view),
                        };

                        // todo(perf): do so locally
                        compose.broadcast(&pkt, system_id).send(&world).unwrap();
                    }
                },
            );

        system!(
            "entity_state_sync",
            world,
            &Compose($),
            &mut Position,
            &Yaw,
            &Pitch,
            &NetworkStreamRef,
            &mut ActiveAnimation,
            &mut PlayerInventory,
            &mut EntityReaction,
        )
            .multi_threaded()
            .kind::<flecs::pipeline::OnStore>()
            .tracing_each_entity(
                info_span!("entity_state_sync"),
                move |entity,
                      (
                          compose,
                          position,
                          yaw,
                          pitch,
                          io,
                          animation,
                          inventory,
                          reaction,
                      )| {
                    let mut run = || {
                        let entity_id = VarInt(entity.minecraft_id());


                        let io = *io;

                        let world = entity.world();

                        let chunk_pos = position.to_chunk();

                        // let health_updated = *prev_health != *health;
                        //
                        // if health_updated {
                        //     let to = *health;
                        //     let from = *prev_health;
                        //
                        //     observer.encode(*health);
                        //     *prev_health = *health;
                        //
                        //     if to < from {
                        //         let pkt = play::EntityDamageS2c {
                        //             entity_id,
                        //             source_type_id: VarInt::default(),
                        //             source_cause_id: VarInt::default(),
                        //             source_direct_id: VarInt::default(),
                        //             source_pos: None,
                        //         };
                        //
                        //         compose.broadcast_local(&pkt, chunk_pos, system_id).send(&world)?;
                        //
                        //         let packet = agnostic::sound(
                        //             ident!("minecraft:entity.player.hurt"),
                        //             **position,
                        //         ).build();
                        //
                        //         compose.broadcast_local(&packet, chunk_pos, system_id).send(&world)?;
                        //     }
                        //
                        //     if *to == 0.0 {
                        //         // send respawn packet
                        //         let pkt = play::PlayerRespawnS2c {
                        //             dimension_type_name: ident!("minecraft:overworld").into(),
                        //             dimension_name: ident!("minecraft:overworld").into(),
                        //             hashed_seed: 0,
                        //             game_mode: GameMode::Survival,
                        //             previous_game_mode: OptGameMode::default(),
                        //             is_debug: false,
                        //             is_flat: false,
                        //             copy_metadata: false,
                        //             last_death_location: None,
                        //             portal_cooldown: VarInt::default(),
                        //         };
                        //         // position.position = PLAYER_SPAWN_POSITION;
                        //         compose.unicast(&pkt, io, system_id, &world)?;
                        //
                        //         **health = 20.0;
                        //
                        //         let show_all = show_all(entity.minecraft_id());
                        //         compose.unicast(show_all.borrow_packet(), io, system_id, &world)?;
                        //     }
                        // }

                        // let entity_flags_updated = *prev_entity_flags != *entity_flags;

                        // if entity_flags_updated {
                        //     observer.encode(*entity_flags);
                        //     *prev_entity_flags = *entity_flags;
                        // }

                        let pkt = play::EntityPositionS2c {
                            entity_id,
                            position: position.as_dvec3(),
                            yaw: ByteAngle::from_degrees(**yaw),
                            pitch: ByteAngle::from_degrees(**pitch),
                            on_ground: false,
                        };

                        compose
                            .broadcast_local(&pkt, chunk_pos, system_id)
                            .exclude(io)
                            .send(&world)?;

                        let pkt = play::EntitySetHeadYawS2c {
                            entity_id,
                            head_yaw: ByteAngle::from_degrees(**yaw),
                        };

                        compose
                            .broadcast(&pkt, system_id)
                            .exclude(io)
                            .send(&world)?;

                        if reaction.velocity != Vec3::ZERO {
                            let velocity = reaction
                                .velocity
                                .to_array()
                                .try_map(|a| {
                                    #[expect(
                                        clippy::cast_possible_truncation,
                                        reason = "https://blog.rust-lang.org/2020/07/16/Rust-1.45.0.html#:~:text=as%20would%20perform%20a%20%22saturating%20cast%22 as is saturating."
                                    )]
                                    let num = (a * 8000.0) as i32;
                                    i16::try_from(num)
                                })
                                .context("velocity too large to fit in i16")?;

                            let velocity =
                                Velocity(velocity);
                            let pkt = play::EntityVelocityUpdateS2c {
                                entity_id,
                                velocity,
                            };

                            compose.unicast(&pkt, io, system_id, &world)?;

                            reaction.velocity = Vec3::ZERO;
                        }

                        for pkt in animation.packets(entity_id) {
                            compose
                                .broadcast_local(&pkt, chunk_pos, system_id)
                                .exclude(io)
                                .send(&world)?;
                        }

                        animation.clear();

                        for slot in &inventory.updated_since_last_tick {
                            let Ok(slot) = u16::try_from(slot) else {
                                error!("failed to convert slot to u16 {slot}");
                                continue;
                            };
                            let item = inventory.get(slot).with_context(|| {
                                format!("failed to get item for slot {slot}")
                            })?;
                            let Ok(slot) = i16::try_from(slot) else {
                                error!("failed to convert slot to i16 {slot}");
                                continue;
                            };
                            let pkt = play::ScreenHandlerSlotUpdateS2c {
                                window_id: 0,
                                state_id: VarInt::default(),
                                slot_idx: slot,
                                slot_data: Cow::Borrowed(item),
                            };
                            compose.unicast(&pkt, io, system_id, &world).context("failed to send inventory update")?;
                        }

                        let cursor = inventory.get_cursor_index();

                        if inventory
                            .updated_since_last_tick
                            .contains(u32::from(cursor))
                            || inventory.hand_slot_updated_since_last_tick
                        {
                            let pkt = play::EntityEquipmentUpdateS2c {
                                entity_id,
                                equipment: vec![EquipmentEntry {
                                    slot: 0,
                                    item: inventory.get_cursor().clone(),
                                }],
                            };

                            compose
                                .broadcast_local(&pkt, chunk_pos, system_id)
                                .exclude(io)
                                .send(&world)
                                .context("failed to send equipment update")?;
                        }

                        inventory.updated_since_last_tick.clear();
                        inventory.hand_slot_updated_since_last_tick = false;

                        anyhow::Ok(())
                    };
                    if let Err(e) = run() {
                        error!("failed to run sync_position: {e}");
                    }
                },
            );
    }
}
