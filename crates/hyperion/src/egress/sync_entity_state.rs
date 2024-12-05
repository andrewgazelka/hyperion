use std::borrow::Cow;

use anyhow::Context;
use flecs_ecs::prelude::*;
use glam::Vec3;
use hyperion_inventory::PlayerInventory;
use hyperion_utils::EntityExt;
use tracing::{error, info_span};
use valence_protocol::{ByteAngle, RawBytes, VarInt, packets::play};

use crate::{
    Prev,
    net::{Compose, ConnectionId},
    simulation::{
        Pitch, Position, Velocity, Xp, Yaw, animation::ActiveAnimation, metadata::MetadataChanges,
    },
    system_registry::{SYNC_ENTITY_POSITION, SystemId},
    util::TracingExt,
};

#[derive(Component)]
pub struct EntityStateSyncModule;

fn track_previous<T: ComponentId + Copy>(world: &World) {
    // we include names so that if we call this multiple times, we don't get multiple observers/systems
    let component_name = std::any::type_name::<T>();
    let observer_name = format!("init_prev_{component_name}");
    let system_name = format!("track_prev_{component_name}");

    world
        .observer_named::<flecs::OnSet, &T>(&observer_name)
        .without::<(Prev, T)>() // we have not set Prev yet
        .each_entity(|entity, value| {
            entity.set_pair::<Prev, T>(*value);
        });

    world
        .system_named::<(&mut (Prev, T), &T)>(system_name.as_str())
        .multi_threaded()
        .kind::<flecs::pipeline::OnStore>()
        .each(|(prev, value)| {
            *prev = *value;
        });
}

impl Module for EntityStateSyncModule {
    fn module(world: &World) {
        let system_id = SYNC_ENTITY_POSITION;

        world
            .system_named::<(
                &Compose,        // (0)
                &ConnectionId,   // (1)
                &mut (Prev, Xp), // (2)
                &mut Xp,         // (3)
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
                        const LANES: usize = 32; // up to AVX512

                        let compose = table.field_unchecked::<Compose>(0);
                        let compose = compose.first().unwrap();

                        let net = table.field_unchecked::<ConnectionId>(1);
                        let net = net.get(..).unwrap();

                        let mut prev_xp = table.field_unchecked::<Xp>(2);
                        let prev_xp = prev_xp.get_mut(..).unwrap();
                        let prev_xp: &mut [u16] =
                            core::slice::from_raw_parts_mut(prev_xp.as_mut_ptr().cast(), count);

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

                                let entity = table.entity(idx);
                                entity.modified::<Xp>();

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
                        compose
                            .broadcast(&pkt, SystemId(9999))
                            .send(&world)
                            .unwrap();
                    }
                },
            );

        system!(
            "active_animation_sync",
            world,
            &Position,
            &Compose($),
            ?&ConnectionId,
            &mut ActiveAnimation,
        )
        .multi_threaded()
        .kind::<flecs::pipeline::OnStore>()
        .tracing_each_entity(
            info_span!("active_animation_sync"),
            move |entity, (position, compose, connection_id, animation)| {
                let io = connection_id.copied();

                let world = entity.world();
                let entity_id = VarInt(entity.minecraft_id());

                let chunk_pos = position.to_chunk();

                for pkt in animation.packets(entity_id) {
                    compose
                        .broadcast_local(&pkt, chunk_pos, system_id)
                        .exclude(io)
                        .send(&world)
                        .unwrap();
                }

                animation.clear();
            },
        );

        system!(
            "player_inventory_sync",
            world,
            &Compose($),
            &mut PlayerInventory,
            &ConnectionId,
        )
        .multi_threaded()
        .kind::<flecs::pipeline::OnStore>()
        .tracing_each_entity(
            info_span!("player_inventory_sync"),
            move |entity, (compose, inventory, io)| {
                let mut run = || {
                    let io = *io;
                    let world = entity.world();

                    for slot in &inventory.updated_since_last_tick {
                        let Ok(slot) = u16::try_from(slot) else {
                            error!("failed to convert slot to u16 {slot}");
                            continue;
                        };
                        let item = inventory
                            .get(slot)
                            .with_context(|| format!("failed to get item for slot {slot}"))?;
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
                        compose
                            .unicast(&pkt, io, system_id, &world)
                            .context("failed to send inventory update")?;
                    }

                    inventory.updated_since_last_tick.clear();
                    inventory.hand_slot_updated_since_last_tick = false;

                    anyhow::Ok(())
                };

                if let Err(e) = run() {
                    error!("Failed to sync player inventory: {}", e);
                }
            },
        );

        system!(
            "entity_velocity_sync",
            world,
            &Compose($),
            &Velocity,
        )
        .multi_threaded()
        .kind::<flecs::pipeline::OnStore>()
        .tracing_each_entity(
            info_span!("entity_velocity_sync"),
            move |entity, (compose, velocity)| {
                let run = || {
                    let entity_id = VarInt(entity.minecraft_id());
                    let world = entity.world();

                    if velocity.velocity != Vec3::ZERO {
                        let pkt = play::EntityVelocityUpdateS2c {
                            entity_id,
                            velocity: (*velocity).try_into()?,
                        };

                        compose.broadcast(&pkt, system_id).send(&world)?;
                    }

                    anyhow::Ok(())
                };

                if let Err(e) = run() {
                    error!("failed to run velocity sync: {e}");
                }
            },
        );

        system!(
            "entity_state_sync",
            world,
            &Compose($),
            &Position,
            &Yaw,
            &Pitch,
            &ConnectionId,
        )
        .multi_threaded()
        .kind::<flecs::pipeline::OnStore>()
        .tracing_each_entity(
            info_span!("entity_state_sync"),
            move |entity, (compose, position, yaw, pitch, io)| {
                let run = || {
                    let entity_id = VarInt(entity.minecraft_id());

                    let io = *io;

                    let world = entity.world();

                    let chunk_pos = position.to_chunk();

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

                    // todo: unsure if we always want to set this
                    let pkt = play::EntitySetHeadYawS2c {
                        entity_id,
                        head_yaw: ByteAngle::from_degrees(**yaw),
                    };

                    compose
                        .broadcast(&pkt, system_id)
                        .exclude(io)
                        .send(&world)?;

                    anyhow::Ok(())
                };
                if let Err(e) = run() {
                    error!("failed to run sync_position: {e}");
                }
            },
        );

        track_previous::<Position>(world);
        track_previous::<Yaw>(world);
        track_previous::<Pitch>(world);
    }
}
