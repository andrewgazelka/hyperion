use std::borrow::Cow;

use flecs_ecs::prelude::*;
use hyperion_inventory::PlayerInventory;
use tracing::trace_span;
use valence_protocol::{
    packets::{play, play::entity_equipment_update_s2c::EquipmentEntry},
    ByteAngle, RawBytes, VarInt,
};

use crate::{
    net::{Compose, NetworkStreamRef},
    simulation::{animation::ActiveAnimation, metadata::Metadata, Position},
    system_registry::SYNC_ENTITY_POSITION,
    util::TracingExt,
};

#[derive(Component)]
pub struct SyncPositionModule;

impl Module for SyncPositionModule {
    fn module(world: &World) {
        let system_id = SYNC_ENTITY_POSITION;

        system!("sync_position", world, &Compose($), &Position, &NetworkStreamRef, &mut Metadata, &mut ActiveAnimation, &mut PlayerInventory)
            .multi_threaded()
            .kind::<flecs::pipeline::OnStore>()
            .tracing_each_entity(
                trace_span!("sync_position"),
                move |entity, (compose, pose, &io, metadata, animation, inventory)| {
                    let entity_id = VarInt(entity.id().0 as i32);

                    let world = entity.world();

                    let pkt = play::EntityPositionS2c {
                        entity_id,
                        position: pose.position.as_dvec3(),
                        yaw: ByteAngle::from_degrees(pose.yaw),
                        pitch: ByteAngle::from_degrees(pose.pitch),
                        on_ground: false,
                    };

                    compose
                        .broadcast(&pkt, system_id)
                        .exclude(io)
                        .send(&world)
                        .unwrap();

                    let pkt = play::EntitySetHeadYawS2c {
                        entity_id,
                        head_yaw: ByteAngle::from_degrees(pose.yaw),
                    };

                    compose
                        .broadcast(&pkt, system_id)
                        .exclude(io)
                        .send(&world)
                        .unwrap();

                    if let Some(view) = metadata.get_and_clear() {
                        let pkt = play::EntityTrackerUpdateS2c {
                            entity_id,
                            tracked_values: RawBytes(&view),
                        };

                        compose
                            .broadcast(&pkt, system_id)
                            .exclude(io)
                            .send(&world)
                            .unwrap();
                    }

                    for pkt in animation.packets(entity_id) {
                        compose
                            .broadcast(&pkt, system_id)
                            .exclude(io)
                            .send(&world)
                            .unwrap();
                    }

                    animation.clear();

                    for slot in inventory.updated_since_last_tick.iter() {
                        let slot = slot as u16;
                        let item = inventory.get(slot).unwrap();
                        let pkt = play::ScreenHandlerSlotUpdateS2c {
                            window_id: 0,
                            state_id: Default::default(),
                            slot_idx: slot as i16,
                            slot_data: Cow::Borrowed(item),
                        };
                        compose.unicast(&pkt, io, system_id, &world).unwrap();
                    }

                    let cursor = inventory.get_cursor_index();

                    if inventory.updated_since_last_tick.contains(cursor as u32) || inventory.hand_slot_updated_since_last_tick {
                        let item = inventory.get_cursor();
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
                            .unwrap();
                    }

                    inventory.updated_since_last_tick.clear();
                    inventory.hand_slot_updated_since_last_tick = false;
                },
            );
    }
}
