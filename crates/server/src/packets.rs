//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>

use evenio::entity::EntityId;
use tracing::{info, warn};
use valence_protocol::{
    decode::PacketFrame,
    math::Vec3,
    packets::play::{
        self, click_slot_c2s::ClickMode, client_command_c2s::ClientCommand,
        player_action_c2s::PlayerAction, player_interact_entity_c2s::EntityInteraction,
    },
    Decode, Packet,
};

use crate::{
    components::FullEntityPose,
    event::{self, AttackType, Pose, SwingArm},
    singleton::player_id_lookup::EntityIdLookup,
    system::ingress::SendElem,
};

pub mod vanilla;
pub mod voicechat;

const fn confirm_teleport(_pkt: &[u8]) {
    // ignore
}

fn full(mut data: &[u8], full_entity_pose: &mut FullEntityPose) -> anyhow::Result<()> {
    const MAX_SPEED: f32 = 100.0;

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
    let d_pos = position - full_entity_pose.position;
    if d_pos.length_squared() > MAX_SPEED.powi(2) {
        // TODO: Add max speed check again. It currently doesn't work because the client is falling
        // into the void until chunks load.
        // bail!("Player is moving too fast max speed: {MAX_SPEED}");
    }

    // todo: analyze clustering
    full_entity_pose.move_to(position);
    full_entity_pose.yaw = yaw;
    full_entity_pose.pitch = pitch;

    Ok(())
}

fn look_and_on_ground(
    mut data: &[u8],
    full_entity_pose: &mut FullEntityPose,
) -> anyhow::Result<()> {
    let pkt = play::LookAndOnGroundC2s::decode(&mut data)?;

    // debug!("look and on ground packet: {:?}", pkt);

    let play::LookAndOnGroundC2s { yaw, pitch, .. } = pkt;

    full_entity_pose.yaw = yaw;
    full_entity_pose.pitch = pitch;

    Ok(())
}

fn position_and_on_ground(
    mut data: &[u8],
    full_entity_pose: &mut FullEntityPose,
) -> anyhow::Result<()> {
    let pkt = play::PositionAndOnGroundC2s::decode(&mut data)?;

    // debug!("position and on ground packet: {:?}", pkt);

    let play::PositionAndOnGroundC2s { position, .. } = pkt;

    // todo: handle like full
    full_entity_pose.move_to(position.as_vec3());

    Ok(())
}

fn update_selected_slot(
    mut data: &[u8],
    sender: &mut Vec<SendElem>,
    player_id: EntityId,
) -> anyhow::Result<()> {
    let pkt = play::UpdateSelectedSlotC2s::decode(&mut data)?;

    let play::UpdateSelectedSlotC2s { slot } = pkt;

    let elem = SendElem::new(player_id, event::UpdateSelectedSlot { slot });
    sender.push(elem);

    Ok(())
}

fn chat_command(
    mut data: &[u8],
    query: &PacketSwitchQuery,
    sender: &mut Vec<SendElem>,
) -> anyhow::Result<()> {
    let pkt = play::CommandExecutionC2s::decode(&mut data)?;

    let event = event::Command {
        raw: pkt.command.0.to_owned(),
    };

    let elem = SendElem::new(query.id, event);
    sender.push(elem);

    Ok(())
}

fn hand_swing(
    mut data: &[u8],
    query: &PacketSwitchQuery,
    sender: &mut Vec<SendElem>,
) -> anyhow::Result<()> {
    let packet = play::HandSwingC2s::decode(&mut data)?;

    let packet = packet.hand;

    let event = SwingArm { hand: packet };

    sender.push(SendElem::new(query.id, event));

    Ok(())
}

fn player_interact_entity(
    mut data: &[u8],
    query: &PacketSwitchQuery,
    id_lookup: &EntityIdLookup,
    from_pos: Vec3,
    sender: &mut Vec<SendElem>,
) -> anyhow::Result<()> {
    let packet = play::PlayerInteractEntityC2s::decode(&mut data)?;

    // attack
    if packet.interact != EntityInteraction::Attack {
        return Ok(());
    }

    let target = packet.entity_id.0;

    if let Some(&target) = id_lookup.get(&target) {
        let elem = SendElem::new(target, event::AttackEntity {
            from_pos,
            from: query.id,
            damage: 10.0,
            source: AttackType::Melee,
        });

        sender.push(elem);
    }

    Ok(())
}

pub struct PacketSwitchQuery<'a> {
    pub id: EntityId,
    pub pose: &'a mut FullEntityPose,
}

fn player_action(
    mut data: &[u8],
    sender: &mut Vec<SendElem>,
    query: &PacketSwitchQuery,
) -> anyhow::Result<()> {
    let packet = play::PlayerActionC2s::decode(&mut data)?;

    let id = query.id;
    let position = packet.position;
    let sequence = packet.sequence.0;

    match packet.action {
        PlayerAction::StartDestroyBlock => {
            let elem = SendElem::new(id, event::BlockStartBreak { position, sequence });
            sender.push(elem);
        }
        PlayerAction::AbortDestroyBlock => {
            let elem = SendElem::new(id, event::BlockAbortBreak { position, sequence });
            sender.push(elem);
        }
        PlayerAction::StopDestroyBlock => {
            let elem = SendElem::new(id, event::BlockFinishBreak { position, sequence });
            sender.push(elem);
        }
        PlayerAction::DropItem => {
            let elem = SendElem::new(id, event::DropItem {
                drop_type: event::DropType::Single,
            });
            sender.push(elem);
        }
        PlayerAction::DropAllItems => {
            let elem = SendElem::new(id, event::DropItem {
                drop_type: event::DropType::All,
            });
            sender.push(elem);
        }
        _ => {}
    }

    Ok(())
}

// for sneaking
fn client_command(
    mut data: &[u8],
    sender: &mut Vec<SendElem>,
    query: &PacketSwitchQuery,
) -> anyhow::Result<()> {
    let packet = play::ClientCommandC2s::decode(&mut data)?;

    let id = query.id;

    match packet.action {
        ClientCommand::StartSneaking => {
            let elem = SendElem::new(id, event::PoseUpdate {
                state: Pose::Sneaking,
            });
            sender.push(elem);
        }
        ClientCommand::StopSneaking => {
            let elem = SendElem::new(id, event::PoseUpdate {
                state: Pose::Standing,
            });
            sender.push(elem);
        }
        _ => {}
    }

    Ok(())
}

pub fn switch(
    raw: &PacketFrame,
    sender: &mut Vec<SendElem>,
    id_lookup: &EntityIdLookup,
    query: &mut PacketSwitchQuery,
) -> anyhow::Result<()> {
    let packet_id = raw.id;
    let data = raw.body.as_ref();

    match packet_id {
        play::HandSwingC2s::ID => hand_swing(data, query, sender)?,
        play::TeleportConfirmC2s::ID => confirm_teleport(data),
        // play::PlayerInteractBlockC2s::ID => player_interact_block(data)?,
        play::ClientCommandC2s::ID => client_command(data, sender, query)?,
        // play::ClientSettingsC2s::ID => client_settings(data, player)?,
        // play::CustomPayloadC2s::ID => custom_payload(data),
        play::FullC2s::ID => full(data, query.pose)?,
        play::PlayerActionC2s::ID => player_action(data, sender, query)?,
        play::PositionAndOnGroundC2s::ID => position_and_on_ground(data, query.pose)?,
        play::LookAndOnGroundC2s::ID => look_and_on_ground(data, query.pose)?,
        // play::ClientCommandC2s::ID => player_command(data),
        // play::UpdatePlayerAbilitiesC2s::ID => update_player_abilities(data)?,
        play::UpdateSelectedSlotC2s::ID => update_selected_slot(data, sender, query.id)?,
        play::PlayerInteractEntityC2s::ID => {
            player_interact_entity(data, query, id_lookup, query.pose.position, sender)?;
        }
        // play::KeepAliveC2s::ID => keep_alive(query.keep_alive)?,
        play::CommandExecutionC2s::ID => chat_command(data, query, sender)?,
        play::ClickSlotC2s::ID => inventory_action(data, sender, query)?,
        _ => {
            // info!("unknown packet id: 0x{:02X}", packet_id)
        }
    }

    Ok(())
}

// for inventory events
fn inventory_action(
    mut data: &[u8],
    sender: &mut Vec<SendElem>,
    query: &PacketSwitchQuery,
) -> anyhow::Result<()> {
    let packet = play::ClickSlotC2s::decode(&mut data)?;

    let play::ClickSlotC2s {
        window_id,
        // todo what is that for? something important?
        slot_idx,
        button,
        mode,
        slot_changes,
        carried_item,
        ..
    } = packet;

    info!("slot changes: {:?}", slot_changes);

    // todo support other windows like chests, etc
    if window_id != 0 {
        warn!("unsupported window id from client: {}", window_id);
        return Ok(());
    };

    let click_type = match mode {
        ClickMode::Click if slot_changes.len() == 1 => {
            let change = slot_changes.iter().next();

            let Some(_) = change else {
                // todo error
                warn!("unexpected empty slot change");
                return Ok(());
            };

            match button {
                0 => event::ClickType::LeftClick {
                    slot: slot_idx,
                    //    slot_change: change,
                },
                1 => event::ClickType::RightClick {
                    slot: slot_idx,
                    //      slot_change: change,
                },
                _ => {
                    // Button no supported for click
                    // todo error
                    warn!("unexpected button for click: {}", button);
                    return Ok(());
                }
            }
        }
        ClickMode::ShiftClick if slot_changes.len() == 2 => {
            // Shift right click is identical behavior to shift left click
            match button {
                0 => event::ClickType::ShiftLeftClick {
                    slot: slot_idx,
                    //            slot_changes: change,
                },
                1 => event::ClickType::ShiftRightClick {
                    slot: slot_idx,
                    //            slot_changes: change,
                },
                _ => {
                    // Button no supported for shift click
                    // todo error
                    warn!("unexpected button for shift click: {}", button);
                    return Ok(());
                }
            }
        }
        ClickMode::Hotbar if slot_changes.len() == 2 => {
            match button {
                // calculate real index
                0..=8 => event::ClickType::HotbarKeyPress {
                    button: button + 36,
                    slot: slot_idx,
                    //    slot_changes: change,
                },
                40 => event::ClickType::OffHandSwap {
                    slot: slot_idx,
                    //          slot_changes: change,
                },
                _ => {
                    // Button no supported for hotbar
                    // todo error
                    warn!("unexpected button for hotbar: {button}");
                    return Ok(());
                }
            }
        }
        ClickMode::CreativeMiddleClick => event::ClickType::CreativeMiddleClick { slot: slot_idx },
        ClickMode::DropKey if slot_changes.len() == 1 => {
            match button {
                0 => event::ClickType::QDrop {
                    slot: slot_idx,
                    //          slot_change: change,
                },
                1 => event::ClickType::QControlDrop {
                    slot: slot_idx,
                    //        slot_change: change,
                },
                _ => {
                    // Button no supported for drop
                    // todo error
                    warn!("unexpected button for drop: {}", button);
                    return Ok(());
                }
            }
        }
        ClickMode::Drag => {
            match button {
                0 => event::ClickType::StartLeftMouseDrag,
                4 => event::ClickType::StartRightMouseDrag,
                8 => event::ClickType::StartMiddleMouseDrag,
                1 => event::ClickType::AddSlotLeftDrag { slot: slot_idx },
                5 => event::ClickType::AddSlotRightDrag { slot: slot_idx },
                9 => event::ClickType::AddSlotMiddleDrag { slot: slot_idx },
                2 => event::ClickType::EndLeftMouseDrag {
                    //slot_changes: slot_changes.iter().cloned().collect(),
                },
                6 => event::ClickType::EndRightMouseDrag {
                    //slot_changes: slot_changes.iter().cloned().collect(),
                },
                10 => event::ClickType::EndMiddleMouseDrag,
                _ => {
                    // Button no supported for drag
                    // todo error
                    warn!("unexpected button for drag: {}", button);
                    return Ok(());
                }
            }
        }
        ClickMode::DoubleClick => {
            match button {
                0 => event::ClickType::DoubleClick {
                    slot: slot_idx,
                    //    slot_changes: slot_changes.iter().cloned().collect(),
                },
                1 => event::ClickType::DoubleClickReverseOrder {
                    slot: slot_idx,
                    //    slot_changes: slot_changes.iter().cloned().collect(),
                },
                _ => {
                    // Button no supported for double click
                    // todo error
                    warn!("unexpected button for double click: {}", button);
                    return Ok(());
                }
            }
        }
        _ => {
            // todo error
            warn!("unexpected click mode or slot change: {:?}", mode);
            return Ok(());
        }
    };

    let id = query.id;

    let event = event::ClickEvent {
        click_type,
        carried_item,
        slot_changes: slot_changes.iter().cloned().collect(),
    };

    let elem = SendElem::new(id, event);
    sender.push(elem);

    Ok(())
}
