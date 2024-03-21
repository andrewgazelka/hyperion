#![allow(clippy::missing_const_for_fn)]
//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>

use anyhow::bail;
use tracing::{debug, warn};
use valence_protocol::{decode::PacketFrame, packets::play, Decode, Packet};

use crate::{FullEntityPose, Player};

fn confirm_teleport(_pkt: &[u8]) {
    // ignore
}

fn client_settings(mut data: &[u8], player: &mut Player) -> anyhow::Result<()> {
    let pkt = play::ClientSettingsC2s::decode(&mut data)?;
    player.locale = Some(pkt.locale.to_owned());
    Ok(())
}

fn custom_payload(_data: &[u8]) {
    // ignore
}

fn full(mut data: &[u8], full_entity_pose: &mut FullEntityPose) -> anyhow::Result<()> {
    let pkt = play::FullC2s::decode(&mut data)?;

    debug!("full packet: {:?}", pkt);

    let play::FullC2s {
        position,
        yaw,
        pitch,
        ..
    } = pkt;

    // check to see if the player is moving too fast
    // if they are, ignore the packet

    const MAX_SPEED: f64 = 100.0;

    if position.distance_squared(full_entity_pose.position) > MAX_SPEED.powi(2) {
        bail!("Player is moving too fast max speed: {MAX_SPEED}");
    }

    // todo: analyze clustering

    full_entity_pose.position = position;
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

fn player_command(mut data: &[u8]) -> anyhow::Result<()> {
    let pkt = play::ClientCommandC2s::decode(&mut data)?;

    debug!("player command packet: {:?}", pkt);

    Ok(())
}

fn position_and_on_ground(
    mut data: &[u8],
    full_entity_pose: &mut FullEntityPose,
) -> anyhow::Result<()> {
    let pkt = play::PositionAndOnGroundC2s::decode(&mut data)?;

    // debug!("position and on ground packet: {:?}", pkt);

    let play::PositionAndOnGroundC2s { position, .. } = pkt;

    full_entity_pose.position = position;

    Ok(())
}

fn update_player_abilities(mut data: &[u8]) -> anyhow::Result<()> {
    let pkt = play::UpdatePlayerAbilitiesC2s::decode(&mut data)?;

    debug!("update player abilities packet: {:?}", pkt);

    Ok(())
}

fn update_selected_slot(mut data: &[u8]) -> anyhow::Result<()> {
    let pkt = play::UpdateSelectedSlotC2s::decode(&mut data)?;

    debug!("update selected slot packet: {:?}", pkt);

    Ok(())
}

pub fn switch(
    raw: PacketFrame,
    player: &mut Player,
    full_entity_pose: &mut FullEntityPose,
) -> anyhow::Result<()> {
    let id = raw.id;
    let data = raw.body;
    let data = &*data;

    match id {
        play::TeleportConfirmC2s::ID => confirm_teleport(data),
        play::ClientSettingsC2s::ID => client_settings(data, player)?,
        play::CustomPayloadC2s::ID => custom_payload(data),
        play::FullC2s::ID => full(data, full_entity_pose)?,
        play::PositionAndOnGroundC2s::ID => position_and_on_ground(data, full_entity_pose)?,
        play::LookAndOnGroundC2s::ID => look_and_on_ground(data, full_entity_pose)?,
        play::ClientCommandC2s::ID => player_command(data)?,
        play::UpdatePlayerAbilitiesC2s::ID => update_player_abilities(data)?,
        play::UpdateSelectedSlotC2s::ID => update_selected_slot(data)?,
        _ => warn!("unknown packet id: 0x{:X}", id),
    }

    Ok(())
}
