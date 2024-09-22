//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>

use std::{borrow::Cow, ops::ControlFlow};

use anyhow::bail;
use bvh_region::aabb::Aabb;
use flecs_ecs::core::{Entity, EntityView, EntityViewGet, World};
use glam::Vec3;
use tracing::{info, instrument, trace, warn};
use valence_protocol::{
    packets::play::{
        self, client_command_c2s::ClientCommand, player_interact_entity_c2s::EntityInteraction,
        player_position_look_s2c::PlayerPositionLookFlags, CustomPayloadC2s,
    },
    Decode, Hand, Packet, VarInt,
};

use super::{
    animation::{self, ActiveAnimation},
    blocks::MinecraftWorld,
    inventory::Inventory,
    metadata::{Metadata, Pose},
    ConfirmBlockSequences, Position,
};
use crate::{
    net::{decoder::BorrowedPacketFrame, Compose, NetworkStreamRef},
    simulation::{event, event::PluginMessage},
    storage::Events,
    system_registry::SystemId,
};

fn full(query: &mut PacketSwitchQuery<'_>, mut data: &[u8]) -> anyhow::Result<()> {
    let pkt = play::FullC2s::decode(&mut data)?;

    let play::FullC2s {
        position,
        yaw,
        pitch,
        ..
    } = pkt;

    // check to see if the player is moving too fast
    // if they are, ignore the packet

    let position = position.as_vec3();
    change_position_or_correct_client(query, position);

    query.pose.yaw = yaw;
    query.pose.pitch = pitch;

    Ok(())
}

// #[instrument(skip_all)]
fn change_position_or_correct_client(query: &mut PacketSwitchQuery<'_>, proposed: Vec3) {
    let pose = &mut *query.pose;

    if try_change_position(proposed, pose, query.blocks) {
        return;
    }

    let pkt = play::PlayerPositionLookS2c {
        // set to previous position
        position: pose.position.as_dvec3(),
        yaw: pose.yaw,
        pitch: pose.pitch,
        flags: PlayerPositionLookFlags::default(),
        teleport_id: VarInt(fastrand::i32(..)),
    };

    query
        .compose
        .unicast(&pkt, query.io_ref, query.system_id, query.world)
        .unwrap();
}

/// Returns true if the position was changed, false if it was not.
fn try_change_position(proposed: Vec3, pose: &mut Position, blocks: &MinecraftWorld) -> bool {
    /// 100.0 m/tick; this is the same as the vanilla server
    const MAX_BLOCKS_PER_TICK: f32 = 100.0;
    let current = pose.position;
    let delta = proposed - current;

    if delta.length_squared() > MAX_BLOCKS_PER_TICK.powi(2) {
        // error!("Player is moving too fast max speed: {MAX_SPEED_PER_TICK}");
        return false;
    }

    let mut proposed_pose = *pose;
    proposed_pose.move_to(proposed);

    let (min, max) = proposed_pose.block_pose_range();

    // so no improper collisions
    let shrunk = proposed_pose.bounding.shrink(0.01);

    let res = blocks.get_blocks(min, max, |pos, block| {
        let pos = Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32);

        for aabb in block.collision_shapes() {
            // convert to our aabb
            let aabb = Aabb::new(aabb.min().as_vec3(), aabb.max().as_vec3());
            let aabb = aabb.move_by(pos);

            if shrunk.collides(&aabb) {
                return ControlFlow::Break(false);
            }
        }

        ControlFlow::Continue(())
    });

    if res.is_break() {
        return false;
    }

    *pose = proposed_pose;

    true
}

fn look_and_on_ground(mut data: &[u8], full_entity_pose: &mut Position) -> anyhow::Result<()> {
    let pkt = play::LookAndOnGroundC2s::decode(&mut data)?;

    // debug!("look and on ground packet: {:?}", pkt);

    let play::LookAndOnGroundC2s { yaw, pitch, .. } = pkt;

    full_entity_pose.yaw = yaw;
    full_entity_pose.pitch = pitch;

    Ok(())
}

fn position_and_on_ground(
    query: &mut PacketSwitchQuery<'_>,
    mut data: &[u8],
) -> anyhow::Result<()> {
    let pkt = play::PositionAndOnGroundC2s::decode(&mut data)?;

    let play::PositionAndOnGroundC2s { position, .. } = pkt;

    change_position_or_correct_client(query, position.as_vec3());

    Ok(())
}

// fn update_selected_slot(
//     mut data: &[u8],
//     world: &'static World,
//     player_id: Entity,
// ) -> anyhow::Result<()> {
//     let pkt = play::UpdateSelectedSlotC2s::decode(&mut data)?;
//
//     let play::UpdateSelectedSlotC2s { slot } = pkt;
//
//     world.send_to(player_id, event::UpdateSelectedSlot { slot });
//
//     Ok(())
// }

fn chat_command(mut data: &[u8], query: &PacketSwitchQuery<'_>) -> anyhow::Result<()> {
    // todo: we could technically remove allocations &[u8] exists until end of tick
    let pkt = play::CommandExecutionC2s::decode(&mut data)?;

    let command = pkt.command.0.to_owned();

    query.events.push(
        event::Command {
            raw: command,
            by: query.id,
        },
        query.world,
    );

    Ok(())
}

fn hand_swing(mut data: &[u8], query: &mut PacketSwitchQuery<'_>) -> anyhow::Result<()> {
    let packet = play::HandSwingC2s::decode(&mut data)?;

    match packet.hand {
        Hand::Main => {
            query.animation.push(animation::Kind::SwingMainArm);
        }
        Hand::Off => {
            query.animation.push(animation::Kind::SwingOffHand);
        }
    }

    Ok(())
}

#[instrument(skip_all)]
fn player_interact_entity(mut data: &[u8], query: &PacketSwitchQuery<'_>) -> anyhow::Result<()> {
    let packet = play::PlayerInteractEntityC2s::decode(&mut data)?;

    let from_pos = query.pose.position;

    // attack
    if packet.interact != EntityInteraction::Attack {
        return Ok(());
    }

    let target = packet.entity_id.0;
    let target = u64::try_from(target).unwrap();

    info!("enqueue attack");
    let target = query.world.entity_from_id(target);

    target.get::<&Events>(|event_queue| {
        event_queue.push(
            event::AttackEntity {
                from_pos,
                from: query.id,
                damage: 0.0,
            },
            query.world,
        );
    });

    Ok(())
}
//
pub struct PacketSwitchQuery<'a> {
    pub id: Entity,
    #[allow(unused)]
    pub view: EntityView<'a>,
    pub compose: &'a Compose,
    pub io_ref: NetworkStreamRef,
    pub pose: &'a mut Position,
    pub events: &'a Events,
    pub world: &'a World,
    pub blocks: &'a MinecraftWorld,
    pub confirm_block_sequences: &'a mut ConfirmBlockSequences,
    pub system_id: SystemId,
    pub inventory: &'a mut Inventory,
    pub metadata: &'a mut Metadata,
    pub animation: &'a mut ActiveAnimation,
}

// i.e., shooting a bow
fn player_action(mut data: &[u8], _query: &PacketSwitchQuery<'_>) -> anyhow::Result<()> {
    let _packet = play::PlayerActionC2s::decode(&mut data)?;

    // todo: implement

    Ok(())
}

// for sneaking
fn client_command(mut data: &[u8], query: &mut PacketSwitchQuery<'_>) -> anyhow::Result<()> {
    let packet = play::ClientCommandC2s::decode(&mut data)?;

    match packet.action {
        ClientCommand::StartSneaking => {
            query.metadata.pose(Pose::Sneaking);
        }
        ClientCommand::StopSneaking | ClientCommand::LeaveBed => {
            query.metadata.pose(Pose::Standing);
        }
        _ => {
            // todo
        }
    }

    Ok(())
}
// // starting to wind up bow
// pub fn player_interact_item(
//     mut data: &[u8],
//     query: &PacketSwitchQuery<'_>,
//     world: &'static World,
// ) -> anyhow::Result<()> {
//     let _packet = play::PlayerInteractItemC2s::decode(&mut data)?;
//
//     let id = query.id;
//
//     world.send_to(id, event::ItemInteract);
//
//     Ok(())
// }

pub fn player_interact_block(
    mut data: &[u8],
    _query: &mut PacketSwitchQuery<'_>,
) -> anyhow::Result<()> {
    let _packet = play::PlayerInteractBlockC2s::decode(&mut data)?;

    // let position = packet.position;
    // let chunk_position = I16Vec2::new((position.x >> 4) as i16, (position.z >> 4) as i16);

    // let Some(entity) = query
    //     .blocks
    //     .get_loaded_chunk_entity(chunk_position, query.world)
    // else {
    //     warn!("player_interact_block: chunk not found");
    //     return Ok(());
    // };
    //
    // if let Some(item) = query.inventory.get_held()
    //     && !item.is_empty()
    // {
    //     let item = item.item;
    //
    //     let Some(block) = BlockKind::from_item_kind(item) else {
    //         warn!("invalid item kind to place: {item:?}");
    //         return Ok(());
    //     };
    //
    //     let block = BlockState::from_kind(block);
    //
    //     let position = position.get_in_direction(packet.face);
    //
    //     // query.blocks.try_set_block_delta(position, block, query.world);
    //
    //     info!("placed block at {position:?}");
    //     return Ok(());
    // }
    //
    //
    // let x = position.x - chunk_start[0];
    // let z = position.z - chunk_start[1];
    //
    // let y = position.y - START_Y;
    //
    // let x = u8::try_from(x).unwrap();
    // let z = u8::try_from(z).unwrap();
    // let y = u16::try_from(y).unwrap();
    //
    // query.confirm_block_sequences.push(packet.sequence.0);

    Ok(())
}

// pub  fn inventory(mut data: &[u8], query: &PacketSwitchQuery<'_>) -> anyhow::Result<()> {
//     // "Set Container Content" packet (ID 0x12)
//     let packet = play::InventoryS2c::decode(&mut data)?;
//
//     let play::InventoryS2c {
//         window_id,
//         state_id,
//         slots,
//         carried_item,
//     } = packet;
//
//     if window_id != 0 {
//         warn!("not player inventory");
//         return Ok(());
//     };
//
//     info!("inventory packet: {window_id} {state_id:?} {slots:?} {carried_item:?}");
//
//
//     Ok(())
// }

pub fn update_selected_slot(
    mut data: &[u8],
    query: &mut PacketSwitchQuery<'_>,
) -> anyhow::Result<()> {
    // "Set Selected Slot" packet (ID 0x0B)
    let packet = play::UpdateSelectedSlotC2s::decode(&mut data)?;

    let play::UpdateSelectedSlotC2s { slot } = packet;

    query.inventory.set_cursor(slot);

    Ok(())
}

pub fn creative_inventory_action(
    mut data: &[u8],
    query: &mut PacketSwitchQuery<'_>,
) -> anyhow::Result<()> {
    // "Creative Inventory Action" packet (ID 0x0C)
    let packet = play::CreativeInventoryActionC2s::decode(&mut data)?;

    let play::CreativeInventoryActionC2s { slot, clicked_item } = packet;

    info!("creative inventory action: {slot} {clicked_item:?}");

    let Ok(slot) = u16::try_from(slot) else {
        warn!("invalid slot {slot}");
        return Ok(());
    };

    query.inventory.set_slot(slot, clicked_item);

    Ok(())
}

pub fn custom_payload(
    mut data: &'static [u8],
    query: &mut PacketSwitchQuery<'_>,
) -> anyhow::Result<()> {
    let packet: CustomPayloadC2s<'static> = play::CustomPayloadC2s::decode(&mut data)?;

    let channel = packet.channel.into_inner();

    let Cow::Borrowed(borrow) = channel else {
        bail!("NO")
    };

    let event = PluginMessage {
        channel: borrow,
        data: packet.data.0 .0,
    };

    // to static
    // let event: PluginMessage<'static> = unsafe { core::mem::transmute(event) };

    query.events.push(event, query.world);

    Ok(())
}

pub fn packet_switch(
    raw: BorrowedPacketFrame<'_>,
    query: &mut PacketSwitchQuery<'_>,
) -> anyhow::Result<()> {
    let packet_id = raw.id;
    let data = raw.body;

    let data: &'static [u8] = unsafe { core::mem::transmute(data) };

    match packet_id {
        play::HandSwingC2s::ID => hand_swing(data, query)?,
        // play::PlayerInteractBlockC2s::ID => player_interact_block(data)?,
        play::ClientCommandC2s::ID => client_command(data, query)?,
        play::FullC2s::ID => full(query, data)?,
        play::PlayerActionC2s::ID => player_action(data, query)?,
        play::PositionAndOnGroundC2s::ID => position_and_on_ground(query, data)?,
        // play::InventoryS2c::ID => inventory(data, query)?,
        play::UpdateSelectedSlotC2s::ID => update_selected_slot(data, query)?,
        play::CreativeInventoryActionC2s::ID => creative_inventory_action(data, query)?,
        play::LookAndOnGroundC2s::ID => look_and_on_ground(data, query.pose)?,
        play::PlayerInteractBlockC2s::ID => player_interact_block(data, query)?,
        play::CustomPayloadC2s::ID => custom_payload(data, query)?,
        // play::UpdatePlayerAbilitiesC2s::ID => update_player_abilities(data)?,
        // play::UpdateSelectedSlotC2s::ID => update_selected_slot(data,
        // world, query.id)?,
        play::PlayerInteractEntityC2s::ID => {
            player_interact_entity(data, query)?;
        }
        // play::PlayerInteractItemC2s::ID => player_interact_item(data, query, world)?,
        // play::KeepAliveC2s::ID => keep_alive(query.keep_alive)?,
        play::CommandExecutionC2s::ID => chat_command(data, query)?,
        // play::ClickSlotC2s::ID => inventory_action(data, world, query)?,
        _ => {
            trace!("unknown packet id: 0x{:02X}", packet_id);
        }
    }

    Ok(())
}

// for inventory events
// fn inventory_action(
//     mut data: &[u8],
//     world: &'static World,
//     query: &PacketSwitchQuery<'_>,
// ) -> anyhow::Result<()> {
//     let packet = play::ClickSlotC2s::decode(&mut data)?;
//
//     let play::ClickSlotC2s {
//         window_id,
//         // todo what is that for?
// something important?
//         slot_idx,
//         button,
//         mode,
//         slot_changes,
//         carried_item,
//         ..
//     } = packet;
//
//     info!("slot changes: {:?}", slot_changes);
//
//     // todo support other windows like chests, etc
//     if window_id != 0 {
//         warn!("unsupported window id from client: {}", window_id);
//         return Ok(());
//     };
//
//     let click_type = match mode {
//         ClickMode::Click if slot_changes.len() == 1 => {
//             let change = slot_changes.iter().next();
//
//             let Some(_) = change else {
//                 // todo error
//                 warn!("unexpected empty slot change");
//                 return Ok(());
//             };
//
//             match button {
//                 0 => event::ClickType::LeftClick {
//                     slot: slot_idx,
//                     //    slot_change: change,
//                 },
//                 1 => event::ClickType::RightClick {
//                     slot: slot_idx,
//                     //      slot_change: change,
//                 },
//                 _ => {
//                     // Button no supported for click
//                     // todo error
//                     warn!("unexpected button for click: {}", button);
//                     return Ok(());
//                 }
//             }
//         }
//         ClickMode::ShiftClick if slot_changes.len() == 2 => {
//             // Shift right click is identical behavior to shift left click
//             match button {
//                 0 => event::ClickType::ShiftLeftClick {
//                     slot: slot_idx,
//                     //            slot_changes: change,
//                 },
//                 1 => event::ClickType::ShiftRightClick {
//                     slot: slot_idx,
//                     //            slot_changes: change,
//                 },
//                 _ => {
//                     // Button no supported for shift click
//                     // todo error
//                     warn!("unexpected button for shift click: {}", button);
//                     return Ok(());
//                 }
//             }
//         }
//         ClickMode::Hotbar if slot_changes.len() == 2 => {
//             match button {
//                 // calculate real index
//                 0..=8 => event::ClickType::HotbarKeyPress {
//                     button: button + 36,
//                     slot: slot_idx,
//                     //    slot_changes: change,
//                 },
//                 40 => event::ClickType::OffHandSwap {
//                     slot: slot_idx,
//                     //          slot_changes: change,
//                 },
//                 _ => {
//                     // Button no supported for hotbar
//                     // todo error
//                     warn!("unexpected button for hotbar: {button}");
//                     return Ok(());
//                 }
//             }
//         }
//         ClickMode::CreativeMiddleClick => event::ClickType::CreativeMiddleClick { slot: slot_idx },
//         ClickMode::DropKey if slot_changes.len() == 1 => {
//             match button {
//                 0 => event::ClickType::QDrop {
//                     slot: slot_idx,
//                     //          slot_change: change,
//                 },
//                 1 => event::ClickType::QControlDrop {
//                     slot: slot_idx,
//                     //        slot_change: change,
//                 },
//                 _ => {
//                     // Button no supported for drop
//                     // todo error
//                     warn!("unexpected button for drop: {}", button);
//                     return Ok(());
//                 }
//             }
//         }
//         ClickMode::Drag => {
//             match button {
//                 0 => event::ClickType::StartLeftMouseDrag,
//                 4 => event::ClickType::StartRightMouseDrag,
//                 8 => event::ClickType::StartMiddleMouseDrag,
//                 1 => event::ClickType::AddSlotLeftDrag { slot: slot_idx },
//                 5 => event::ClickType::AddSlotRightDrag { slot: slot_idx },
//                 9 => event::ClickType::AddSlotMiddleDrag { slot: slot_idx },
//                 2 => event::ClickType::EndLeftMouseDrag {
//                     //slot_changes: slot_changes.iter().cloned().collect(),
//                 },
//                 6 => event::ClickType::EndRightMouseDrag {
//                     //slot_changes: slot_changes.iter().cloned().collect(),
//                 },
//                 10 => event::ClickType::EndMiddleMouseDrag,
//                 _ => {
//                     // Button no supported for drag
//                     // todo error
//                     warn!("unexpected button for drag: {}", button);
//                     return Ok(());
//                 }
//             }
//         }
//         ClickMode::DoubleClick => {
//             match button {
//                 0 => event::ClickType::DoubleClick {
//                     slot: slot_idx,
//                     //    slot_changes: slot_changes.iter().cloned().collect(),
//                 },
//                 1 => event::ClickType::DoubleClickReverseOrder {
//                     slot: slot_idx,
//                     //    slot_changes: slot_changes.iter().cloned().collect(),
//                 },
//                 _ => {
//                     // Button no supported for double click
//                     // todo error
//                     warn!("unexpected button for double click: {}", button);
//                     return Ok(());
//                 }
//             }
//         }
//         _ => {
//             // todo error
//             warn!("unexpected click mode or slot change: {:?}", mode);
//             return Ok(());
//         }
//     };
//
//     let id = query.id;
//
//     let event = event::ClickEvent {
//         click_type,
//         carried_item,
//         slot_changes: slot_changes.iter().cloned().collect(),
//     };
//
//     world.send_to(id, event);
//
//     Ok(())
// }
