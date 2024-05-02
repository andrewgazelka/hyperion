#![expect(
    unused,
    reason = "there are still many changes that need to be made to this file"
)]
#![allow(
    clippy::missing_docs_in_private_items,
    reason = "most of this is self-explanatory"
)]

//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>

use std::str::FromStr;

use anyhow::ensure;
use evenio::{entity::EntityId, query::Query};
use valence_protocol::{
    decode::PacketFrame,
    math::Vec3,
    packets::{
        play,
        play::{
            client_command_c2s::ClientCommand, player_action_c2s::PlayerAction,
            player_interact_entity_c2s::EntityInteraction,
        },
    },
    Decode, Packet,
};

use crate::{
    components::{FullEntityPose, ImmuneStatus, KeepAlive, Vitals},
    event,
    event::{AttackEntity, AttackType, Pose, SwingArm},
    global::Global,
    singleton::player_id_lookup::EntityIdLookup,
    system::ingress::SendElem,
};

pub mod vanilla;
pub mod voicechat;

const fn confirm_teleport(_pkt: &[u8]) {
    // ignore
}

const fn custom_payload(_data: &[u8]) {
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

const fn player_command(data: &[u8]) {
    // let pkt = play::ClientCommandC2s::decode(&mut data)?;

    // debug!("player command packet: {:?}", pkt);
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

fn update_player_abilities(mut data: &[u8]) -> anyhow::Result<()> {
    let pkt = play::UpdatePlayerAbilitiesC2s::decode(&mut data)?;

    // debug!("update player abilities packet: {:?}", pkt);

    Ok(())
}

fn update_selected_slot(mut data: &[u8]) -> anyhow::Result<()> {
    let pkt = play::UpdateSelectedSlotC2s::decode(&mut data)?;

    // debug!("update selected slot packet: {:?}", pkt);

    Ok(())
}

fn keep_alive(player: &mut KeepAlive) -> anyhow::Result<()> {
    ensure!(!player.unresponded, "keep alive sent unexpectedly");
    player.unresponded = false;
    // player.ping = player.last_keep_alive_sent.elapsed();
    Ok(())
}

#[derive(Debug, Copy, Clone)]
enum HybridPos {
    Absolute(f32),
    Relative(f32),
}

// impl parse

impl FromStr for HybridPos {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((l, r)) = s.split_once('~') {
            ensure!(l.is_empty(), "expected ~ to be at the start of the string");

            if r.is_empty() {
                return Ok(Self::Relative(0.0));
            }

            let num = r.parse()?;
            return Ok(Self::Relative(num));
        }

        let num = s.parse()?;
        Ok(Self::Absolute(num))
    }
}

fn chat_command(
    mut data: &[u8],
    query: &PacketSwitchQuery,
    // query: PacketSwitchQuery,
    sender: &mut Vec<SendElem>,
) -> anyhow::Result<()> {
    const BASE_RADIUS: f32 = 4.0;
    let pkt = play::CommandExecutionC2s::decode(&mut data)?;

    let event = event::Command {
        by: query.id,
        raw: pkt.command.0.to_owned(),
    };

    sender.push(event.into());

    Ok(())
}

fn hand_swing(
    mut data: &[u8],
    query: &PacketSwitchQuery,
    sender: &mut Vec<SendElem>,
) -> anyhow::Result<()> {
    let packet = play::HandSwingC2s::decode(&mut data)?;

    let packet = packet.hand;

    let event = SwingArm {
        target: query.id,
        hand: packet,
    };

    sender.push(event.into());

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
        sender.push(
            AttackEntity {
                target,
                from_pos,
                from: query.id,
                damage: 10.0,
                source: AttackType::Melee,
            }
            .into(),
        );
    }

    Ok(())
}

pub struct PacketSwitchQuery<'a> {
    pub id: EntityId,
    pub pose: &'a mut FullEntityPose,
    pub vitals: &'a mut Vitals,
    pub keep_alive: &'a mut KeepAlive,
    pub immunity: &'a mut ImmuneStatus,
}

/// i.e., doors, etc
fn player_interact_block(mut data: &[u8]) -> anyhow::Result<()> {
    let packet = play::PlayerInteractBlockC2s::decode(&mut data)?;

    // todo!()
    Ok(())
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
            sender.push(
                event::BlockStartBreak {
                    by: id,
                    position,
                    sequence,
                }
                .into(),
            );
        }
        PlayerAction::AbortDestroyBlock => {
            sender.push(
                event::BlockAbortBreak {
                    by: id,
                    position,
                    sequence,
                }
                .into(),
            );
        }
        PlayerAction::StopDestroyBlock => {
            sender.push(
                event::BlockFinishBreak {
                    by: id,
                    position,
                    sequence,
                }
                .into(),
            );
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
            sender.push(
                event::PoseUpdate {
                    target: id,
                    state: Pose::Sneaking,
                }
                .into(),
            );
        }
        ClientCommand::StopSneaking => {
            sender.push(
                event::PoseUpdate {
                    target: id,
                    state: Pose::Standing,
                }
                .into(),
            );
        }
        _ => {}
    }

    Ok(())
}

pub fn switch(
    raw: PacketFrame,
    global: &Global,
    sender: &mut Vec<SendElem>,
    id_lookup: &EntityIdLookup,
    query: &mut PacketSwitchQuery,
) -> anyhow::Result<()> {
    let packet_id = raw.id;
    let data = raw.body;
    let data = &*data;

    match packet_id {
        play::HandSwingC2s::ID => hand_swing(data, query, sender)?,
        play::TeleportConfirmC2s::ID => confirm_teleport(data),
        play::PlayerInteractBlockC2s::ID => player_interact_block(data)?,
        play::ClientCommandC2s::ID => client_command(data, sender, query)?,
        // play::ClientSettingsC2s::ID => client_settings(data, player)?,
        // play::CustomPayloadC2s::ID => custom_payload(data),
        play::FullC2s::ID => full(data, query.pose)?,
        play::PlayerActionC2s::ID => player_action(data, sender, query)?,
        play::PositionAndOnGroundC2s::ID => position_and_on_ground(data, query.pose)?,
        play::LookAndOnGroundC2s::ID => look_and_on_ground(data, query.pose)?,
        // play::ClientCommandC2s::ID => player_command(data),
        // play::UpdatePlayerAbilitiesC2s::ID => update_player_abilities(data)?,
        // play::UpdateSelectedSlotC2s::ID => update_selected_slot(data)?,
        play::PlayerInteractEntityC2s::ID => {
            player_interact_entity(data, query, id_lookup, query.pose.position, sender)?;
        }
        // play::KeepAliveC2s::ID => keep_alive(query.keep_alive)?,
        play::CommandExecutionC2s::ID => chat_command(data, query, sender)?,
        _ => {
            // info!("unknown packet id: 0x{:02X}", packet_id)
        }
    }

    Ok(())
}
