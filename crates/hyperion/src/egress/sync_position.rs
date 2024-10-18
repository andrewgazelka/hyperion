use std::borrow::Cow;

use anyhow::Context;
use flecs_ecs::prelude::*;
use glam::Vec3;
use hyperion_inventory::PlayerInventory;
use hyperion_utils::EntityExt;
use tracing::{error, trace_span};
use valence_protocol::{
    game_mode::OptGameMode,
    ident,
    packets::{play, play::entity_equipment_update_s2c::EquipmentEntry},
    sound::{SoundCategory, SoundId},
    ByteAngle, GameMode, RawBytes, VarInt, Velocity,
};

use crate::{
    egress::metadata::show_all,
    net::{Compose, NetworkStreamRef},
    simulation::{
        animation::ActiveAnimation, metadata::Metadata, EntityReaction, Health, Position,
    },
    system_registry::SYNC_ENTITY_POSITION,
    util::TracingExt,
};

#[derive(Component)]
pub struct SyncPositionModule;

impl Module for SyncPositionModule {
    fn module(world: &World) {
        let system_id = SYNC_ENTITY_POSITION;

        system!(
            "sync_position",
            world,
            &Compose($),
            &mut Position,
            &NetworkStreamRef,
            &mut Metadata,
            &mut ActiveAnimation,
            &mut PlayerInventory,
            &mut EntityReaction,
            &mut Health
        )
        .multi_threaded()
        .kind::<flecs::pipeline::OnStore>()
        .tracing_each_entity(
            trace_span!("sync_position"),
            move |entity,
                  elems: (
                &Compose,
                &mut Position,
                &NetworkStreamRef,
                &mut Metadata,
                &mut ActiveAnimation,
                &mut PlayerInventory,
                &mut EntityReaction,
                &mut Health,
            )| {
                let (compose, pose, io, metadata, animation, inventory, reaction, health) = elems;

                let mut run = || {
                    let entity_id = VarInt(entity.minecraft_id());

                    let io = *io;

                    let world = entity.world();

                    let position = pose.position.as_dvec3();

                    let pkt = play::EntityPositionS2c {
                        entity_id,
                        position: pose.position.as_dvec3(),
                        yaw: ByteAngle::from_degrees(pose.yaw),
                        pitch: ByteAngle::from_degrees(pose.pitch),
                        on_ground: false,
                    };

                    compose
                        .broadcast_local(&pkt, pose.chunk_pos(), system_id)
                        .exclude(io)
                        .send(&world)?;

                    // let pkt = play::EntitySetHeadYawS2c {
                    //     entity_id,
                    //     head_yaw: ByteAngle::from_degrees(pose.yaw),
                    // };
                    // 
                    // compose
                    //     .broadcast(&pkt, system_id)
                    //     .exclude(io)
                    //     .send(&world)?;

                    if reaction.velocity != Vec3::ZERO {
                        let velocity = reaction
                            .velocity
                            .to_array()
                            .try_map(|a| {
                                #[expect(clippy::cast_possible_truncation, reason = "https://blog.rust-lang.org/2020/07/16/Rust-1.45.0.html#:~:text=as%20would%20perform%20a%20%22saturating%20cast%22 as is saturating.")]
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

                    if let Some(value) = health.pop_updated() {
                        let from = value.from;
                        let to = value.to;

                        // not sure health update is needed
                        // let pkt = play::HealthUpdateS2c {
                        //     health: to,
                        //     food: VarInt(20),
                        //     food_saturation: 10.0,
                        // };
                        //
                        // compose.unicast(&pkt, io, system_id, &world).unwrap();
                        metadata.health(to);

                        if to < from {
                            let pkt = play::EntityDamageS2c {
                                entity_id,
                                source_type_id: VarInt::default(),
                                source_cause_id: VarInt::default(),
                                source_direct_id: VarInt::default(),
                                source_pos: None,
                            };

                            compose.broadcast(&pkt, system_id).send(&world)?;

                            // Play a sound when an entity is damaged
                            let ident = ident!("minecraft:entity.player.hurt");
                            let pkt = play::PlaySoundS2c {
                                id: SoundId::Direct {
                                    id: ident.into(),
                                    range: None,
                                },
                                position: (position * 8.0).as_ivec3(),
                                volume: 1.0,
                                pitch: 1.0,
                                seed: fastrand::i64(..),
                                category: SoundCategory::Player,
                            };
                            compose.broadcast(&pkt, system_id).send(&world)?;
                        }

                        if to == 0.0 {
                            // send respawn packet
                            let pkt = play::PlayerRespawnS2c {
                                dimension_type_name: ident!("minecraft:overworld").into(),
                                dimension_name: ident!("minecraft:overworld").into(),
                                hashed_seed: 0,
                                game_mode: GameMode::Adventure,
                                previous_game_mode: OptGameMode::default(),
                                is_debug: false,
                                is_flat: false,
                                copy_metadata: false,
                                last_death_location: None,
                                portal_cooldown: VarInt::default(),
                            };
                            // pose.position = PLAYER_SPAWN_POSITION;
                            compose.unicast(&pkt, io, system_id, &world)?;

                            health.reset();

                            let show_all = show_all(entity.minecraft_id());
                            compose.unicast(show_all.borrow_packet(), io, system_id, &world)?;
                        }
                    }

                    if let Some(view) = metadata.get_and_clear() {
                        let pkt = play::EntityTrackerUpdateS2c {
                            entity_id,
                            tracked_values: RawBytes(&view),
                        };

                        compose.broadcast(&pkt, system_id).send(&world)?;
                    }

                    for pkt in animation.packets(entity_id) {
                        compose
                            .broadcast(&pkt, system_id)
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
                            .broadcast(&pkt, system_id)
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
