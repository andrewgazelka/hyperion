//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>

use std::borrow::Cow;

use anyhow::{bail, Context};
use bvh_region::aabb::Aabb;
use flecs_ecs::core::{Entity, EntityView, World};
use glam::{IVec3, Vec3};
use hyperion_utils::EntityExt;
use tracing::{info, instrument, trace, warn};
use valence_generated::block::{BlockKind, BlockState, PropName};
use valence_protocol::{
    nbt,
    packets::play::{
        self, click_slot_c2s::SlotChange, client_command_c2s::ClientCommand,
        player_action_c2s::PlayerAction, player_interact_entity_c2s::EntityInteraction,
        player_position_look_s2c::PlayerPositionLookFlags,
    },
    Decode, Hand, Packet, VarInt,
};
use valence_text::IntoText;

use super::{
    animation::{self, ActiveAnimation},
    block_bounds,
    blocks::Blocks,
    metadata::{Metadata, Pose},
    ConfirmBlockSequences, EntitySize, Position,
};
use crate::{
    net::{decoder::BorrowedPacketFrame, Compose, NetworkStreamRef},
    simulation::{aabb, event, event::PluginMessage, Pitch, Yaw},
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

    query.yaw.yaw = yaw as f16;
    query.pitch.pitch = pitch as f16;

    Ok(())
}

// #[instrument(skip_all)]
fn change_position_or_correct_client(query: &mut PacketSwitchQuery<'_>, proposed: Vec3) {
    let pose = &mut *query.position;

    if let Err(e) = try_change_position(proposed, pose, *query.size, query.blocks) {
        // Send error message to player
        let msg = format!("§c{e}");
        let pkt = play::GameMessageS2c {
            chat: msg.into_cow_text(),
            overlay: false,
        };

        if let Err(e) = query
            .compose
            .unicast(&pkt, query.io_ref, query.system_id, query.world)
        {
            warn!("Failed to send error message to player: {e}");
        }

        // Correct client position
        let pkt = play::PlayerPositionLookS2c {
            position: pose.position.as_dvec3(),
            yaw: query.yaw.yaw as f32,
            pitch: query.pitch.pitch as f32,
            flags: PlayerPositionLookFlags::default(),
            teleport_id: VarInt(fastrand::i32(..)),
        };

        if let Err(e) = query
            .compose
            .unicast(&pkt, query.io_ref, query.system_id, query.world)
        {
            warn!("Failed to correct client position: {e}");
        }
    }
}

/// Returns true if the position was changed, false if it was not.
/// The vanilla server has a max speed of 100 blocks per tick.
/// However, we are much more conservative.
const MAX_BLOCKS_PER_TICK: f32 = 30.0;

/// Returns true if the position was changed, false if it was not.
///
/// Movement validity rules:
/// ```text
///   From  |   To    | Allowed
/// --------|---------|--------
/// in  🧱  | in  🧱  |   ✅
/// in  🧱  | out 🌫️  |   ✅  
/// out 🌫️  | in  🧱  |   ❌
/// out 🌫️  | out 🌫️  |   ✅
/// ```
/// Only denies movement if starting outside a block and moving into a block.
/// This prevents players from glitching into blocks while allowing them to move out.
fn try_change_position(
    proposed: Vec3,
    position: &mut Position,
    size: EntitySize,
    blocks: &Blocks,
) -> anyhow::Result<()> {
    is_within_speed_limits(**position, proposed)?;

    // Only check collision if we're starting outside a block
    if !has_collision(position, size, blocks) && has_collision(&proposed, size, blocks) {
        return Err(anyhow::anyhow!("Cannot move into solid blocks"));
    }

    **position = proposed;
    Ok(())
}

fn is_within_speed_limits(current: Vec3, proposed: Vec3) -> anyhow::Result<()> {
    let delta = proposed - current;
    if delta.length_squared() > MAX_BLOCKS_PER_TICK.powi(2) {
        return Err(anyhow::anyhow!(
            "Moving too fast! Maximum speed is {MAX_BLOCKS_PER_TICK} blocks per tick"
        ));
    }
    Ok(())
}

fn has_collision(position: &Vec3, size: EntitySize, blocks: &Blocks) -> bool {
    use std::ops::ControlFlow;

    let (min, max) = block_bounds(*position, size);
    let shrunk = aabb(*position, size).shrink(0.01);

    let res = blocks.get_blocks(min, max, |pos, block| {
        let pos = Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32);

        for aabb in block.collision_shapes() {
            let aabb = Aabb::new(aabb.min().as_vec3(), aabb.max().as_vec3());
            let aabb = aabb.move_by(pos);

            if shrunk.collides(&aabb) {
                return ControlFlow::Break(false);
            }
        }

        ControlFlow::Continue(())
    });

    res.is_break()
}

fn look_and_on_ground(mut data: &[u8], query: &mut PacketSwitchQuery<'_>) -> anyhow::Result<()> {
    let pkt = play::LookAndOnGroundC2s::decode(&mut data)?;

    let play::LookAndOnGroundC2s { yaw, pitch, .. } = pkt;

    **query.yaw = yaw as f16;
    **query.pitch = pitch as f16;

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

    // attack
    if packet.interact != EntityInteraction::Attack {
        return Ok(());
    }

    let target = packet.entity_id.0;
    let target = Entity::from_minecraft_id(target);

    query.events.push(
        event::AttackEntity {
            origin: query.id,
            target,
            damage: 1.0,
        },
        query.world,
    );

    Ok(())
}
//
pub struct PacketSwitchQuery<'a> {
    pub id: Entity,
    pub view: EntityView<'a>,
    pub compose: &'a Compose,
    pub io_ref: NetworkStreamRef,
    pub position: &'a mut Position,
    pub yaw: &'a mut Yaw,
    pub pitch: &'a mut Pitch,
    pub size: &'a EntitySize,
    pub events: &'a Events,
    pub world: &'a World,
    pub blocks: &'a Blocks,
    pub confirm_block_sequences: &'a mut ConfirmBlockSequences,
    pub system_id: SystemId,
    pub inventory: &'a mut hyperion_inventory::PlayerInventory,
    pub metadata: &'a mut Metadata,
    pub animation: &'a mut ActiveAnimation,
    pub crafting_registry: &'a hyperion_crafting::CraftingRegistry,
}

// i.e., shooting a bow, digging a block, etc
fn player_action(mut data: &[u8], query: &PacketSwitchQuery<'_>) -> anyhow::Result<()> {
    let packet = play::PlayerActionC2s::decode(&mut data)?;

    let sequence = packet.sequence.0;
    let position = IVec3::new(packet.position.x, packet.position.y, packet.position.z);

    match packet.action {
        PlayerAction::StopDestroyBlock => {
            let event = event::DestroyBlock {
                position,
                from: query.id,
                sequence,
            };

            query.events.push(event, query.world);
        }
        _ => bail!("unimplemented"),
    }

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
    query: &mut PacketSwitchQuery<'_>,
) -> anyhow::Result<()> {
    let packet = play::PlayerInteractBlockC2s::decode(&mut data)?;

    // PlayerInteractBlockC2s contains:
    // - hand: Hand (enum: MainHand or OffHand)
    // - position: BlockPos (x, y, z coordinates of the block)
    // - face: Direction (enum: Down, Up, North, South, West, East)
    // - cursor_position: Vec3 (x, y, z coordinates of cursor on the block face)
    // - inside_block: bool (whether the player's head is inside a block)
    // - sequence: VarInt (sequence number for this interaction)

    query.confirm_block_sequences.push(packet.sequence.0);

    let interacted_block_pos = packet.position;
    let interacted_block_pos_vec = IVec3::new(
        interacted_block_pos.x,
        interacted_block_pos.y,
        interacted_block_pos.z,
    );

    let Some(interacted_block) = query.blocks.get_block(interacted_block_pos_vec) else {
        return Ok(());
    };

    if interacted_block.get(PropName::Open).is_some() {
        // Toggle the open state of a door
        // todo: place block instead of toggling door if the player is crouching and holding a
        // block

        query.events.push(
            event::ToggleDoor {
                position: interacted_block_pos_vec,
                from: query.id,
                sequence: packet.sequence.0,
            },
            query.world,
        );
    } else {
        // Attempt to place a block

        let held = query.inventory.get_cursor();

        if held.is_empty() {
            return Ok(());
        }

        let Some(nbt) = &held.nbt else {
            return Ok(());
        };

        let Some(can_place_on) = nbt.get("CanPlaceOn") else {
            return Ok(());
        };

        let nbt::Value::List(can_place_on) = can_place_on else {
            return Ok(());
        };

        let nbt::list::List::String(can_place_on) = can_place_on else {
            return Ok(());
        };

        let kind_name = interacted_block.to_kind().to_str();
        let kind_name = format!("minecraft:{kind_name}");

        if !can_place_on.iter().any(|s| s == &kind_name) {
            return Ok(());
        }

        let kind = held.item;

        let Some(block_kind) = BlockKind::from_item_kind(kind) else {
            warn!("invalid item kind to place: {kind:?}");
            return Ok(());
        };

        let block_state = BlockState::from_kind(block_kind);

        let position = interacted_block_pos.get_in_direction(packet.face);
        let position = IVec3::new(position.x, position.y, position.z);

        let position_dvec3 = position.as_vec3();

        // todo(hack): technically players can do some crazy position stuff to abuse this probably
        // let player_aabb = query.position.bounding.shrink(0.01);
        let player_aabb = aabb(**query.position, *query.size).shrink(0.01);

        let collides_player = block_state
            .collision_shapes()
            .map(|aabb| Aabb::new(aabb.min().as_vec3(), aabb.max().as_vec3()))
            .map(|aabb| aabb.move_by(position_dvec3))
            .any(|block_aabb| player_aabb.collides(&block_aabb));

        if collides_player {
            return Ok(());
        }

        query.inventory.take_one_held();

        query.events.push(
            event::PlaceBlock {
                position,
                from: query.id,
                sequence: packet.sequence.0,
                block: block_state,
            },
            query.world,
        );
    }

    Ok(())
}

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

    query.inventory.set(slot, clicked_item)?;

    Ok(())
}

pub fn custom_payload(
    mut data: &'static [u8],
    query: &mut PacketSwitchQuery<'_>,
) -> anyhow::Result<()> {
    let packet: play::CustomPayloadC2s<'static> = play::CustomPayloadC2s::decode(&mut data)?;

    let channel = packet.channel.into_inner();

    let Cow::Borrowed(borrow) = channel else {
        bail!("NO")
    };

    let event = PluginMessage {
        channel: borrow,
        data: packet.data.0 .0,
    };

    query.events.push(event, query.world);

    Ok(())
}

fn click_slot(mut data: &[u8], query: &mut PacketSwitchQuery<'_>) -> anyhow::Result<()> {
    let pkt = play::ClickSlotC2s::decode(&mut data)?;

    // todo(security): validate the player can do this. This is a MAJOR security issue.
    // as players will be able to spawn items in their inventory wit current logic.
    for SlotChange { idx, stack } in pkt.slot_changes.iter() {
        let idx = u16::try_from(*idx).context("slot index is negative")?;
        query.inventory.set(idx, stack.clone())?;
    }

    let item = query.inventory.crafting_item(query.crafting_registry);
    let set_item_pkt = play::ScreenHandlerSlotUpdateS2c {
        window_id: 0,
        state_id: VarInt(0),
        slot_idx: 0, // crafting result
        slot_data: Cow::Owned(item),
    };

    query
        .compose
        .unicast(&set_item_pkt, query.io_ref, query.system_id, query.world)?;

    Ok(())
}

fn chat_message(mut data: &'static [u8], query: &PacketSwitchQuery<'_>) -> anyhow::Result<()> {
    // todo: we could technically remove allocations &[u8] exists until end of tick
    let pkt = play::ChatMessageC2s::decode(&mut data)?;
    let msg = pkt.message.0;

    query
        .events
        .push(event::ChatMessage { msg, by: query.id }, query.world);

    Ok(())
}

pub fn packet_switch(
    raw: BorrowedPacketFrame<'_>,
    query: &mut PacketSwitchQuery<'_>,
) -> anyhow::Result<()> {
    let packet_id = raw.id;
    let data = raw.body;

    // ideally we wouldn't have to do this. The lifetime is the same as the entire tick.
    // as the data is bump-allocated and reset occurs at the end of the tick
    let data: &'static [u8] = unsafe { core::mem::transmute(data) };

    match packet_id {
        play::ChatMessageC2s::ID => chat_message(data, query)?,
        play::ClickSlotC2s::ID => click_slot(data, query)?,
        play::ClientCommandC2s::ID => client_command(data, query)?,
        play::CommandExecutionC2s::ID => chat_command(data, query)?,
        play::CreativeInventoryActionC2s::ID => creative_inventory_action(data, query)?,
        play::CustomPayloadC2s::ID => custom_payload(data, query)?,
        play::FullC2s::ID => full(query, data)?,
        play::HandSwingC2s::ID => hand_swing(data, query)?,
        play::LookAndOnGroundC2s::ID => look_and_on_ground(data, query)?,
        play::PlayerActionC2s::ID => player_action(data, query)?,
        play::PlayerInteractBlockC2s::ID => player_interact_block(data, query)?,
        play::PlayerInteractEntityC2s::ID => player_interact_entity(data, query)?,
        play::PositionAndOnGroundC2s::ID => position_and_on_ground(query, data)?,
        play::UpdateSelectedSlotC2s::ID => update_selected_slot(data, query)?,
        _ => trace!("unknown packet id: 0x{:02X}", packet_id),
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
