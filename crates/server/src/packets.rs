#![expect(
    unused_variables,
    reason = "there are still many changes that need to be made to this file"
)]
#![allow(
    clippy::missing_docs_in_private_items,
    reason = "most of this is self-explanatory"
)]

//! <https://wiki.vg/index.php?title=Protocol&oldid=18375>

pub mod def;

use std::str::FromStr;

use anyhow::{bail, ensure};
use bvh::aabb::Aabb;
use evenio::entity::EntityId;
use tracing::debug;
use valence_protocol::{
    decode::PacketFrame,
    math::Vec3,
    packets::{play, play::player_interact_entity_c2s::EntityInteraction},
    Decode, Packet,
};

use crate::{
    global::Global, singleton::player_id_lookup::PlayerIdLookup, system::IngressSender, Absorption,
    AttackEntity, FullEntityPose, InitEntity, KillAllEntities, Player, PlayerState, Regeneration,
    SwingArm,
};

const fn confirm_teleport(_pkt: &[u8]) {
    // ignore
}

fn client_settings(mut data: &[u8], player: &mut Player) -> anyhow::Result<()> {
    let pkt = play::ClientSettingsC2s::decode(&mut data)?;
    player.locale = Some(pkt.locale.to_owned());
    Ok(())
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
        bail!("Player is moving too fast max speed: {MAX_SPEED}");
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

fn keep_alive(player: &mut Player) -> anyhow::Result<()> {
    ensure!(
        !player.unresponded_keep_alive,
        "keep alive sent unexpectedly"
    );
    player.unresponded_keep_alive = false;
    player.ping = player.last_keep_alive_sent.elapsed();
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
    global: &Global,
    player: &mut Player,
    full_entity_pose: &FullEntityPose,
    sender: &mut IngressSender,
) -> anyhow::Result<()> {
    const BASE_RADIUS: f32 = 4.0;
    let pkt = play::CommandExecutionC2s::decode(&mut data)?;

    let mut cmd = pkt.command.0.split(' ');

    let first = cmd.next();
    let tick = global.tick.unsigned_abs();

    if first == Some("ka") {
        sender.send(KillAllEntities);
    } else if first == Some("golden_apple") {
        player.state.update(|state| {
            let PlayerState::Alive {
                absorption,
                regeneration,
                ..
            } = state
            else {
                return;
            };
            *absorption = Absorption {
                end_tick: tick + 2400,
                bonus_health: 4.0,
            };
            *regeneration = Regeneration {
                end_tick: tick + 100,
            };
        });
    } else if first == Some("heal") {
        let args: Vec<_> = cmd.collect();
        let [amount] = args.as_slice() else {
            anyhow::bail!("expected 1 number");
        };
        player.heal(amount.parse()?);
    } else if first == Some("hurt") {
        let args: Vec<_> = cmd.collect();
        let [amount] = args.as_slice() else {
            anyhow::bail!("expected 1 number");
        };
        player.hurt(global, amount.parse()?);
    } else if first == Some("spawn") {
        let args: Vec<_> = cmd.collect();

        let loc = full_entity_pose.position;

        let [x, y, z] = match args.as_slice() {
            &[x, y, z] => [x.parse()?, y.parse()?, z.parse()?],
            [x] => {
                let count = x.parse()?;

                // normalize over the number
                #[expect(clippy::cast_possible_truncation, reason = "sqrt of f64 is f32")]
                let radius = BASE_RADIUS * f64::from(count).sqrt() as f32;

                for _ in 0..count {
                    // spawn in 100 block radius
                    let x = (rand::random::<f32>() - 0.5).mul_add(radius, loc.x);
                    let y = loc.y;
                    let z = (rand::random::<f32>() - 0.5).mul_add(radius, loc.z);

                    sender.send(InitEntity {
                        pose: FullEntityPose {
                            position: Vec3::new(x, y, z),
                            yaw: 0.0,
                            pitch: 0.0,
                            bounding: Aabb::create(Vec3::new(x, y, z), 0.6, 1.8),
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

        sender.send(InitEntity {
            pose: FullEntityPose {
                position: Vec3::new(x, y, z),
                yaw: 0.0,
                pitch: 0.0,
                bounding: Aabb::create(Vec3::new(x, y, z), 0.6, 1.8),
            },
        });
    }

    Ok(())
}

fn hand_swing(mut data: &[u8], id: EntityId, sender: &mut IngressSender) -> anyhow::Result<()> {
    let packet = play::HandSwingC2s::decode(&mut data)?;

    let packet = packet.hand;

    let event = SwingArm {
        target: id,
        hand: packet,
    };

    sender.send(event);

    Ok(())
}

fn player_interact_entity(
    mut data: &[u8],
    id_lookup: &PlayerIdLookup,
    from_pos: Vec3,
    sender: &mut IngressSender,
) -> anyhow::Result<()> {
    let packet = play::PlayerInteractEntityC2s::decode(&mut data)?;

    // attack
    if packet.interact != EntityInteraction::Attack {
        return Ok(());
    }

    let target = packet.entity_id.0;

    if let Some(&target) = id_lookup.inner.get(&target) {
        sender.send(AttackEntity { target, from_pos });
    }

    Ok(())
}

pub fn switch(
    raw: PacketFrame,
    global: &Global,
    id: EntityId,
    player: &mut Player,
    full_entity_pose: &mut FullEntityPose,
    id_lookup: &PlayerIdLookup,
    sender: &mut IngressSender,
) -> anyhow::Result<()> {
    let packet_id = raw.id;
    let data = raw.body;
    let data = &*data;

    match packet_id {
        play::HandSwingC2s::ID => hand_swing(data, id, sender)?,
        play::TeleportConfirmC2s::ID => confirm_teleport(data),
        play::ClientSettingsC2s::ID => client_settings(data, player)?,
        play::CustomPayloadC2s::ID => custom_payload(data),
        play::FullC2s::ID => full(data, full_entity_pose)?,
        play::PositionAndOnGroundC2s::ID => position_and_on_ground(data, full_entity_pose)?,
        play::LookAndOnGroundC2s::ID => look_and_on_ground(data, full_entity_pose)?,
        play::ClientCommandC2s::ID => player_command(data),
        play::UpdatePlayerAbilitiesC2s::ID => update_player_abilities(data)?,
        play::UpdateSelectedSlotC2s::ID => update_selected_slot(data)?,
        play::PlayerInteractEntityC2s::ID => {
            player_interact_entity(data, id_lookup, full_entity_pose.position, sender)?;
        }
        play::KeepAliveC2s::ID => keep_alive(player)?,
        play::CommandExecutionC2s::ID => {
            chat_command(data, global, player, full_entity_pose, sender)?;
        }
        _ => debug!("unknown packet id: 0x{:02X}", packet_id),
    }

    Ok(())
}
