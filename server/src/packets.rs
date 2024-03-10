//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>


use anyhow::bail;
use valence_protocol::{decode::PacketFrame, packets::play, Decode, Packet};

use crate::{FullEntityPose, Player};

fn confirm_teleport(pkt: play::TeleportConfirmC2s) {
    // ignore
}

fn message_ack(pkt: play::MessageAcknowledgmentC2s) {
    // ignore
}

fn chat_command(pkt: play::CommandExecutionC2s) {

    // ignore
}

fn client_settings(pkt: play::ClientSettingsC2s, player: &mut Player) {
    player.locale = Some(pkt.locale.to_string());
}

fn custom_payload(pkt: play::CustomPayloadC2s, player: &mut Player) {
    // ignore
}

fn full(pkt: play::FullC2s, full_entity_pose: &mut FullEntityPose) -> anyhow::Result<()> {
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





enum Action {

}

fn switch(
    raw: PacketFrame,
    player: &mut Player,
    full_entity_pose: &mut FullEntityPose,
) -> anyhow::Result<()> {
    let id = raw.id;
    let data = raw.body;
    let mut data = &data[..];
    let data = &mut data;

    match id {
        play::TeleportConfirmC2s::ID => {
            let pkt = play::TeleportConfirmC2s::decode(data)?;
            confirm_teleport(pkt);
        }
        play::ClientSettingsC2s::ID => {
            let pkt = play::ClientSettingsC2s::decode(data)?;
            client_settings(pkt, player);
        }
        play::CustomPayloadC2s::ID => {
            let pkt = play::CustomPayloadC2s::decode(data)?;
            custom_payload(pkt, player);
        }
        play::FullC2s::ID => {
            let pkt = play::FullC2s::decode(data)?;
            full(pkt, full_entity_pose)?;
        }
        _ => bail!("unknown packet id: {id}"),
    }

    Ok(())
}
