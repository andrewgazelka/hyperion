use std::borrow::Cow;

use anyhow::Context;
use flecs_ecs::prelude::*;
use glam::Vec3;
use hyperion_inventory::PlayerInventory;
use hyperion_utils::EntityExt;
use tracing::error;
use valence_protocol::{
    RawBytes, VarInt,
    packets::play::{self},
};

use crate::{
    Prev,
    net::{Compose, ConnectionId},
    simulation::{Position, Velocity, Xp, animation::ActiveAnimation, metadata::MetadataChanges},
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
                let system = table.system();
                while table.next() {
                    let count = table.count();

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

                                compose.unicast(&packet, *net, system).unwrap();
                            },
                        );
                    }
                }
            });

        system!("entity_metadata_sync", world, &Compose($), &mut MetadataChanges)
            .multi_threaded()
            .kind::<flecs::pipeline::OnStore>()
            .each_iter(move |it, row, (compose, metadata_changes)| {
                let system = it.system();
                let entity = it.entity(row);
                let entity_id = VarInt(entity.minecraft_id());

                let metadata = metadata_changes.get_and_clear();

                if let Some(view) = metadata {
                    let pkt = play::EntityTrackerUpdateS2c {
                        entity_id,
                        tracked_values: RawBytes(&view),
                    };

                    // todo(perf): do so locally
                    compose.broadcast(&pkt, system).send().unwrap();
                }
            });

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
        .each_iter(
            move |it, row, (position, compose, connection_id, animation)| {
                let io = connection_id.copied();

                let entity = it.entity(row);
                let system = it.system();

                let entity_id = VarInt(entity.minecraft_id());

                let chunk_pos = position.to_chunk();

                for pkt in animation.packets(entity_id) {
                    compose
                        .broadcast_local(&pkt, chunk_pos, system)
                        .exclude(io)
                        .send()
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
        .each_iter(move |it, _, (compose, inventory, io)| {
            let mut run = || {
                let io = *io;
                let system = it.system();

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
                        .unicast(&pkt, io, system)
                        .context("failed to send inventory update")?;
                }

                inventory.updated_since_last_tick.clear();
                inventory.hand_slot_updated_since_last_tick = false;

                anyhow::Ok(())
            };

            if let Err(e) = run() {
                error!("Failed to sync player inventory: {}", e);
            }
        });

        // Add a new system specifically for projectiles (arrows)
        system!(
            "projectile_sync",
            world,
            &Compose($),
            &Position,
            &(Prev, Position),
            &mut Velocity,
        )
        .multi_threaded()
        .kind::<flecs::pipeline::PreStore>()
        .each_iter(
            move |it, row, (compose, position, previous_position, velocity)| {
                let entity = it.entity(row);
                let system = it.system();

                let entity_id = VarInt(entity.minecraft_id());
                let chunk_pos = position.to_chunk();

                let position_delta = **position - **previous_position;
                let needs_teleport = position_delta.abs().max_element() >= 8.0;
                let changed_position = **position != **previous_position;

                if changed_position && !needs_teleport {
                    let pkt = play::MoveRelativeS2c {
                        entity_id,
                        #[allow(clippy::cast_possible_truncation)]
                        delta: (position_delta * 4096.0).to_array().map(|x| x as i16),
                        on_ground: velocity.velocity == Vec3::ZERO,
                    };

                    compose
                        .broadcast_local(&pkt, chunk_pos, system)
                        .send()
                        .unwrap();
                }

                // Sync velocity if non-zero
                if velocity.velocity != Vec3::ZERO {
                    let pkt = play::EntityVelocityUpdateS2c {
                        entity_id,
                        velocity: (*velocity).try_into().unwrap_or_else(|_| {
                            Velocity::ZERO
                                .try_into()
                                .expect("failed to convert velocity to i16")
                        }),
                    };

                    compose.broadcast(&pkt, system).send().unwrap();
                }
            },
        );

        track_previous::<Position>(world);
    }
}
