//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>

use anyhow::bail;
use valence_protocol::{decode::PacketFrame, packets::play, Decode, Packet};

use crate::{FullEntityPose, Player};

fn confirm_teleport(pkt: &[u8]) {
    // ignore
}

fn message_ack(pkt: play::MessageAcknowledgmentC2s) {
    // ignore
}

fn chat_command(pkt: play::CommandExecutionC2s) {
    // ignore
}

fn client_settings(mut data: &[u8], player: &mut Player) -> anyhow::Result<()> {
    let pkt = play::ClientSettingsC2s::decode(&mut data)?;
    player.locale = Some(pkt.locale.to_string());
    Ok(())
}

fn custom_payload(data: &[u8]) {
    // ignore
}

fn full(mut data: &[u8], full_entity_pose: &mut FullEntityPose) -> anyhow::Result<()> {
    let pkt = play::FullC2s::decode(&mut data)?;

    let play::FullC2s {
        position,
        yaw,
        pitch,
        on_ground,
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

fn chat_msg(pkt: play::ChatMessageC2s) {
    // pkt.
    // ignore for now
}

enum Action {}

fn switch(
    raw: PacketFrame,
    player: &mut Player,
    full_entity_pose: &mut FullEntityPose,
) -> anyhow::Result<()> {
    let id = raw.id;
    let data = raw.body;
    let data = &data[..];

    match id {
        play::TeleportConfirmC2s::ID => confirm_teleport(data),
        play::ClientSettingsC2s::ID => client_settings(data, player)?,
        play::CustomPayloadC2s::ID => custom_payload(data),
        play::FullC2s::ID => full(data, full_entity_pose)?,
        _ => bail!("unknown packet id: {id}"),
    }

    Ok(())
}
