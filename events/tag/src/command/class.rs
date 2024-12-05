use std::borrow::Cow;

use clap::Parser;
use flecs_ecs::core::{Entity, EntityViewGet, World, WorldGet};
use hyperion::{
    egress::{
        metadata::show_all,
        player_join::{PlayerListActions, PlayerListEntry, PlayerListS2c},
    },
    net::{Compose, ConnectionId, DataBundle, agnostic},
    simulation::{Pitch, Position, Xp, Yaw},
    system_registry::SystemId,
    valence_ident::ident,
    valence_protocol::{
        BlockPos, GameMode, VarInt,
        game_mode::OptGameMode,
        packets::play::{
            self, PlayerRespawnS2c, player_position_look_s2c::PlayerPositionLookFlags,
        },
        profile::Property,
    },
};
use hyperion_clap::{CommandPermission, MinecraftCommand};
use hyperion_rank_tree::{Class, Team};
use hyperion_utils::EntityExt;

#[derive(Parser, CommandPermission, Debug)]
#[command(name = "class")]
#[command_permission(group = "Normal")]
pub struct ClassCommand {
    class: Class,
    team: Team,
}
impl MinecraftCommand for ClassCommand {
    fn execute(self, world: &World, caller: Entity) {
        let class_param = self.class;
        let team_param = self.team;

        world.get::<&Compose>(|compose| {
            let caller = caller.entity_view(world);
            caller.get::<(
                &ConnectionId,
                &hyperion::simulation::Uuid,
                &Position,
                &Yaw,
                &Pitch,
                &mut Team,
                &mut Class,
                &Xp,
            )>(|(stream, uuid, position, yaw, pitch, team, class, xp)| {
                if *team == team_param && *class == class_param {
                    let chat_pkt = agnostic::chat("§cYou’re already using this class!");

                    let mut bundle = DataBundle::new(compose);

                    bundle.add_packet(&chat_pkt, world).unwrap();

                    bundle.send(world, *stream, SystemId(0)).unwrap();

                    return;
                }

                if *team != team_param {
                    *team = team_param;
                    caller.modified::<Team>();
                }

                if *class != class_param {
                    *class = class_param;
                    caller.modified::<Class>();
                }

                let minecraft_id = caller.minecraft_id();
                let mut bundle = DataBundle::new(compose);

                let mut position_block = position.floor().as_ivec3();
                position_block.y -= 1;

                // Add bundle splitter so these are all received at once
                bundle.add_packet(&play::BundleSplitterS2c, world).unwrap();

                // Set respawn position to player's position
                bundle
                    .add_packet(
                        &play::PlayerSpawnPositionS2c {
                            position: BlockPos::new(
                                position_block.x,
                                position_block.y,
                                position_block.z,
                            ),
                            // todo: seems to not do anything; perhaps angle is different than yaw?
                            // regardless doesn't matter as we teleport to the correct position
                            // later anyway
                            angle: **yaw,
                        },
                        world,
                    )
                    .unwrap();

                // Remove player info
                bundle
                    .add_packet(
                        &play::PlayerRemoveS2c {
                            uuids: Cow::Borrowed(&[uuid.0]),
                        },
                        world,
                    )
                    .unwrap();

                // Destroy player entity
                bundle
                    .add_packet(
                        &play::EntitiesDestroyS2c {
                            entity_ids: Cow::Borrowed(&[VarInt(minecraft_id)]),
                        },
                        world,
                    )
                    .unwrap();

                let skin = class.skin();
                let property = Property {
                    name: "textures".to_string(),
                    value: skin.textures.clone(),
                    signature: Some(skin.signature.clone()),
                };

                let property = &[property];

                // Add player back with new skin
                bundle
                    .add_packet(
                        &PlayerListS2c {
                            actions: PlayerListActions::default().with_add_player(true),
                            entries: Cow::Borrowed(&[PlayerListEntry {
                                player_uuid: uuid.0,
                                username: Cow::Borrowed("Player"),
                                properties: Cow::Borrowed(property),
                                chat_data: None,
                                listed: true,
                                ping: 20,
                                game_mode: GameMode::Survival,
                                display_name: None,
                            }]),
                        },
                        world,
                    )
                    .unwrap();

                // Respawn player
                bundle
                    .add_packet(
                        &PlayerRespawnS2c {
                            dimension_type_name: ident!("minecraft:overworld").into(),
                            dimension_name: ident!("minecraft:overworld").into(),
                            hashed_seed: 0,
                            game_mode: GameMode::Survival,
                            previous_game_mode: OptGameMode::default(),
                            is_debug: false,
                            is_flat: false,
                            copy_metadata: false,
                            last_death_location: None,
                            portal_cooldown: VarInt::default(),
                        },
                        world,
                    )
                    .unwrap();

                // look and teleport to more accurate position than full-block respawn position
                bundle
                    .add_packet(
                        &play::PlayerPositionLookS2c {
                            position: position.as_dvec3(),
                            yaw: **yaw,
                            pitch: **pitch,
                            flags: PlayerPositionLookFlags::default(),
                            teleport_id: VarInt(fastrand::i32(..)),
                        },
                        world,
                    )
                    .unwrap();

                let visual = xp.get_visual();

                let xp_packet = play::ExperienceBarUpdateS2c {
                    bar: visual.prop,
                    level: VarInt(i32::from(visual.level)),
                    total_xp: VarInt::default(),
                };

                bundle.add_packet(&xp_packet, world).unwrap();

                let msg = format!("Setting rank to {class:?} with yaw {yaw:?}");
                let chat = agnostic::chat(msg);
                bundle.add_packet(&chat, world).unwrap();

                let show_all = show_all(minecraft_id);
                bundle.add_packet(show_all.borrow_packet(), world).unwrap();

                bundle.add_packet(&play::BundleSplitterS2c, world).unwrap();

                bundle.send(world, *stream, SystemId(0)).unwrap();
            });
        });
    }
}
