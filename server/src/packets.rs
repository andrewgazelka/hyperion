#![allow(clippy::missing_const_for_fn)]
#![allow(unused_variables)]
//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>

use std::str::FromStr;

use anyhow::{bail, ensure};
use evenio::event::Sender;
use tracing::{debug, warn};
use valence_protocol::{decode::PacketFrame, math::DVec3, packets::play, Decode, Packet};

use crate::{
    bounding_box::BoundingBox, FullEntityPose, InitEntity, KickPlayer, KillAllEntities, Player,
};

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

fn player_command(data: &[u8]) {
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

    full_entity_pose.position = position;

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

#[derive(Debug, Copy, Clone)]
enum HybridPos {
    Absolute(f64),
    Relative(f64),
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
    player: &mut Player,
    full_entity_pose: &FullEntityPose,
    sender: &mut Sender<(KickPlayer, InitEntity, KillAllEntities)>,
) -> anyhow::Result<()> {
    let pkt = play::CommandExecutionC2s::decode(&mut data)?;

    let mut cmd = pkt.command.0.split(' ');

    let first = cmd.next();

    if first == Some("ka") {
        sender.send(KillAllEntities);
    }

    if first == Some("spawn") {
        let args: Vec<_> = cmd.collect();

        let loc = full_entity_pose.position;

        let [x, y, z] = match args.as_slice() {
            &[x, y, z] => [x.parse()?, y.parse()?, z.parse()?],
            [x] => {
                let count = x.parse()?;

                const BASE_RADIUS: f64 = 4.0;

                // normalize over the number
                let radius = BASE_RADIUS * (count as f64).sqrt();

                for _ in 0..count {
                    // spawn in 100 block radius
                    let x = (rand::random::<f64>() - 0.5).mul_add(radius, loc.x);
                    let y = loc.y;
                    let z = (rand::random::<f64>() - 0.5).mul_add(radius, loc.z);

                    sender.send(InitEntity {
                        pose: FullEntityPose {
                            position: DVec3::new(x, y, z),
                            yaw: 0.0,
                            pitch: 0.0,
                            bounding: BoundingBox::create(DVec3::new(x, y, z), 0.6, 1.8),
                        },
                    });
                }

                return Ok(());
            }
            [] => [HybridPos::Relative(0.0); 3],
            _ => bail!("expected 3 numbers"),
        };

        let x = match x {
            HybridPos::Absolute(x) => x,
            HybridPos::Relative(x) => loc.x + x,
        };

        let y = match y {
            HybridPos::Absolute(y) => y,
            HybridPos::Relative(y) => loc.y + y,
        };

        let z = match z {
            HybridPos::Absolute(z) => z,
            HybridPos::Relative(z) => loc.z + z,
        };

        player
            .packets
            .writer
            .send_chat_message(&format!("Spawning zombie at {x}, {y}, {z}"))?;

        sender.send(InitEntity {
            pose: FullEntityPose {
                position: DVec3::new(x, y, z),
                yaw: 0.0,
                pitch: 0.0,
                bounding: BoundingBox::create(DVec3::new(x, y, z), 0.6, 1.8),
            },
        });
    }

    Ok(())
}

pub fn switch(
    raw: PacketFrame,
    player: &mut Player,
    full_entity_pose: &mut FullEntityPose,
    sender: &mut Sender<(KickPlayer, InitEntity, KillAllEntities)>,
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
        play::ClientCommandC2s::ID => player_command(data),
        play::UpdatePlayerAbilitiesC2s::ID => update_player_abilities(data)?,
        play::UpdateSelectedSlotC2s::ID => update_selected_slot(data)?,
        play::KeepAliveC2s::ID => (), // todo: implement
        play::CommandExecutionC2s::ID => chat_command(data, player, full_entity_pose, sender)?,
        _ => warn!("unknown packet id: 0x{:02X}", id),
    }

    Ok(())
}
