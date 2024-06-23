//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>

use std::ops::ControlFlow;

use bvh_region::aabb::Aabb;
use flecs_ecs::core::{Entity, EntityView, World};
use glam::Vec3;
use tracing::{info, instrument, trace, warn};
use valence_protocol::{
    decode::PacketFrame,
    packets::play::{
        self, client_command_c2s::ClientCommand, player_interact_entity_c2s::EntityInteraction,
        player_position_look_s2c::PlayerPositionLookFlags,
    },
    Decode, Packet, VarInt,
};

use crate::{
    component::{blocks::Blocks, Pose},
    event,
    event::{Allocator, EventQueue, Posture},
    net::{Compose, NetworkStreamRef},
    singleton::player_id_lookup::EntityIdLookup,
};

pub mod vanilla;
// pub mod voicechat;

// const fn confirm_teleport(_pkt: &[u8]) {
//     // ignore
// }

fn full(query: &mut PacketSwitchQuery, mut data: &[u8], blocks: &Blocks) -> anyhow::Result<()> {
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
    change_position_or_correct_client(query, position, blocks);

    query.pose.yaw = yaw;
    query.pose.pitch = pitch;

    Ok(())
}

#[instrument(skip_all)]
fn change_position_or_correct_client(
    query: &mut PacketSwitchQuery,
    proposed: Vec3,
    blocks: &Blocks,
) {
    let pose = &mut *query.pose;

    if try_change_position(proposed, pose, blocks) {
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

    query.compose.unicast(&pkt, query.io_ref).unwrap();
}

/// Returns true if the position was changed, false if it was not.
fn try_change_position(proposed: Vec3, pose: &mut Pose, blocks: &Blocks) -> bool {
    /// 100.0 m/tick; this is the same as the vanilla server
    const MAX_BLOCKS_PER_TICK: f32 = 5.0;
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

fn look_and_on_ground(mut data: &[u8], full_entity_pose: &mut Pose) -> anyhow::Result<()> {
    let pkt = play::LookAndOnGroundC2s::decode(&mut data)?;

    // debug!("look and on ground packet: {:?}", pkt);

    let play::LookAndOnGroundC2s { yaw, pitch, .. } = pkt;

    full_entity_pose.yaw = yaw;
    full_entity_pose.pitch = pitch;

    Ok(())
}

fn position_and_on_ground(
    query: &mut PacketSwitchQuery,
    mut data: &[u8],
    blocks: &Blocks,
) -> anyhow::Result<()> {
    let pkt = play::PositionAndOnGroundC2s::decode(&mut data)?;

    // debug!("position and on ground packet: {:?}", pkt);

    let play::PositionAndOnGroundC2s { position, .. } = pkt;

    change_position_or_correct_client(query, position.as_vec3(), blocks);

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

// fn chat_command(
//     mut data: &[u8],
//     query: &PacketSwitchQuery,
//     world: &'static World,
// ) -> anyhow::Result<()> {
//     let pkt = play::CommandExecutionC2s::decode(&mut data)?;
//
//     let event = event::Command {
//         raw: pkt.command.0.to_owned(),
//     };
//
//     world.send_to(query.id, event);
//
//     Ok(())
// }

fn hand_swing(mut data: &[u8], query: &PacketSwitchQuery) -> anyhow::Result<()> {
    let packet = play::HandSwingC2s::decode(&mut data)?;

    let packet = packet.hand;

    let event = event::SwingArm { hand: packet };

    query.event_queue.push(event, query.allocator).unwrap();

    Ok(())
}

#[instrument(skip_all)]
fn player_interact_entity(
    mut data: &[u8],
    query: &PacketSwitchQuery,
    id_lookup: &EntityIdLookup,
    from_pos: Vec3,
    world: &World,
) -> anyhow::Result<()> {
    let packet = play::PlayerInteractEntityC2s::decode(&mut data)?;

    // attack
    if packet.interact != EntityInteraction::Attack {
        return Ok(());
    }

    let target = packet.entity_id.0;

    let Some(target) = id_lookup.get(&target).as_deref().copied() else {
        // no target
        warn!(
            "no target for {target}.. id_lookup haas {} entries",
            id_lookup.len()
        );
        return Ok(());
    };

    info!("enqueue attack");
    let target = world.entity_from_id(target);

    target.get::<&EventQueue>(|event_queue| {
        event_queue
            .push(
                event::AttackEntity {
                    from_pos,
                    from: query.id,
                    damage: 0.0,
                },
                query.allocator,
            )
            .unwrap();
    });

    Ok(())
}
//
pub struct PacketSwitchQuery<'a> {
    pub id: Entity,
    #[allow(unused)]
    pub view: EntityView<'a>,
    pub compose: &'a Compose,
    pub io_ref: &'a NetworkStreamRef,
    pub pose: &'a mut Pose,
    pub allocator: &'a Allocator,
    pub event_queue: &'a EventQueue,
}
// // i.e., shooting a bow
// fn player_action(
//     mut data: &[u8],
//     world: &'static World,
//     query: &PacketSwitchQuery,
// ) -> anyhow::Result<()> {
//     let packet = play::PlayerActionC2s::decode(&mut data)?;
//
//     info!("player action: {:?}", packet);
//
//     let id = query.id;
//     let position = packet.position;
//     let sequence = packet.sequence.0;
//
//     match packet.action {
//         PlayerAction::StartDestroyBlock => {
//             // let elem = SendElem::new(id, event::BlockStartBreak { position, sequence });
//             // world.push(elem);
//             world.send_to(id, event::BlockStartBreak { position, sequence });
//         }
//         PlayerAction::AbortDestroyBlock => {
//             // let elem = SendElem::new(id, event::BlockAbortBreak { position, sequence });
//             // world.push(elem);
//             world.send_to(id, event::BlockAbortBreak { position, sequence });
//         }
//         PlayerAction::StopDestroyBlock => {
//             // let elem = SendElem::new(id, event::BlockFinishBreak { position, sequence });
//             // world.push(elem);
//             world.send_to(id, event::BlockFinishBreak { position, sequence });
//         }
//         PlayerAction::ReleaseUseItem => {
//             world.send_to(id, event::ReleaseItem);
//         }
//         _ => {}
//     }
//
//     Ok(())
// }

// for sneaking
fn client_command(mut data: &[u8], query: &PacketSwitchQuery) -> anyhow::Result<()> {
    let packet = play::ClientCommandC2s::decode(&mut data)?;

    match packet.action {
        ClientCommand::StartSneaking => {
            query
                .event_queue
                .push(
                    event::PostureUpdate {
                        state: Posture::Sneaking,
                    },
                    query.allocator,
                )
                .unwrap();
        }
        ClientCommand::StopSneaking => {
            query
                .event_queue
                .push(
                    event::PostureUpdate {
                        state: Posture::Standing,
                    },
                    query.allocator,
                )
                .unwrap();
        }
        _ => {}
    }

    Ok(())
}
// // starting to wind up bow
// pub fn player_interact_item(
//     mut data: &[u8],
//     query: &PacketSwitchQuery,
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

pub fn packet_switch(
    raw: &PacketFrame,
    world: &World,
    id_lookup: &EntityIdLookup,
    query: &mut PacketSwitchQuery,
    blocks: &Blocks,
) -> anyhow::Result<()> {
    let packet_id = raw.id;
    let data = raw.body.as_ref();

    match packet_id {
        play::HandSwingC2s::ID => hand_swing(data, query)?,
        // play::PlayerInteractBlockC2s::ID => player_interact_block(data)?,
        play::ClientCommandC2s::ID => client_command(data, query)?,
        play::FullC2s::ID => full(query, data, blocks)?,
        // play::PlayerActionC2s::ID => player_action(data, world, query)?,
        play::PositionAndOnGroundC2s::ID => position_and_on_ground(query, data, blocks)?,
        play::LookAndOnGroundC2s::ID => look_and_on_ground(data, query.pose)?,
        // play::ClientCommandC2s::ID => player_command(data),
        // play::UpdatePlayerAbilitiesC2s::ID => update_player_abilities(data)?,
        // play::UpdateSelectedSlotC2s::ID => update_selected_slot(data, world, query.id)?,
        play::PlayerInteractEntityC2s::ID => {
            player_interact_entity(data, query, id_lookup, query.pose.position, world)?;
        }
        // play::PlayerInteractItemC2s::ID => player_interact_item(data, query, world)?,
        // play::KeepAliveC2s::ID => keep_alive(query.keep_alive)?,
        // play::CommandExecutionC2s::ID => chat_command(data, query, world)?,
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
//     query: &PacketSwitchQuery,
// ) -> anyhow::Result<()> {
//     let packet = play::ClickSlotC2s::decode(&mut data)?;
//
//     let play::ClickSlotC2s {
//         window_id,
//         // todo what is that for? something important?
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
