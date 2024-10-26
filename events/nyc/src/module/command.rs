use std::borrow::Cow;

use flecs_ecs::prelude::*;
use hyperion::{
    egress::player_join::{PlayerListActions, PlayerListEntry, PlayerListS2c},
    net::{Compose, NetworkStreamRef},
    simulation::{
        blocks::Blocks,
        command::{get_root_command, Command, Parser},
        event, Health, InGameName, Position, Uuid,
    },
    storage::EventQueue,
    system_registry::SystemId,
    uuid,
    valence_protocol::{
        self,
        game_mode::OptGameMode,
        ident,
        math::IVec3,
        nbt,
        packets::play::{
            self, command_tree_s2c::StringArg, player_abilities_s2c::PlayerAbilitiesFlags,
            player_position_look_s2c::PlayerPositionLookFlags, PlayerAbilitiesS2c,
        },
        text::IntoText,
        BlockState, GameMode, ItemKind, ItemStack, VarInt,
    },
};
use hyperion_inventory::PlayerInventory;
use parse::Stat;
use tracing::{debug, trace_span};

use crate::{
    component::team::Team,
    module::{attack::CombatStats, command::parse::ParsedCommand, level::Level},
};

pub mod parse;

fn add_command(world: &World, command: Command, parent: Entity) -> Entity {
    world.entity().set(command).child_of_id(parent).id()
}

pub fn add_to_tree(world: &World) {
    let root_command = get_root_command();

    // add to tree
    add_command(world, Command::literal("team"), root_command);
    add_command(world, Command::literal("zombie"), root_command);
    add_command(world, Command::literal("give"), root_command);
    add_command(world, Command::literal("upgrade"), root_command);

    let speed = add_command(world, Command::literal("speed"), root_command);
    add_command(
        world,
        Command::argument("amount", Parser::Float {
            min: Some(0.0),
            max: Some(1024.0),
        }),
        speed,
    );

    let stat = add_command(world, Command::literal("stat"), root_command);
    let stat_child = add_command(
        world,
        Command::argument("type", Parser::String(StringArg::SingleWord)),
        stat,
    );
    add_command(
        world,
        Command::argument("amount", Parser::Float {
            min: Some(0.0),
            max: Some(1024.0),
        }),
        stat_child,
    );

    let health = add_command(world, Command::literal("health"), root_command);
    add_command(
        world,
        Command::argument("amount", Parser::Float {
            min: Some(0.0),
            max: None,
        }),
        health,
    );

    add_command(world, Command::literal("tp"), root_command);

    // Add tphere command
    add_command(world, Command::literal("tphere"), root_command);
}

struct CommandContext<'a> {
    entity: Entity,
    position: &'a mut Position,
    stream: NetworkStreamRef,
    team: &'a mut Team,
    compose: &'a Compose,
    blocks: &'a mut Blocks,
    world: &'a World,
    system_id: SystemId,
    uuid: uuid::Uuid,
    name: &'a InGameName,
    inventory: &'a mut PlayerInventory,
    level: &'a mut Level,
    health: &'a mut Health,
}

fn process_command(command: &ParsedCommand, context: &mut CommandContext<'_>) {
    match command {
        ParsedCommand::Speed(amount) => handle_speed_command(*amount, context),
        ParsedCommand::Team => handle_team_command(context),
        ParsedCommand::Zombie => handle_zombie_command(context),
        ParsedCommand::Dirt { x, y, z } => handle_dirt_command(*x, *y, *z, context),
        ParsedCommand::Give => handle_give_command(context),
        ParsedCommand::Upgrade => handle_upgrade_command(context),
        ParsedCommand::Stats(stat, amount) => handle_stats(*stat, *amount, context),
        ParsedCommand::Health(amount) => handle_health_command(*amount, context),
        ParsedCommand::TpHere => handle_tphere_command(context),
        ParsedCommand::Tp { x, y, z } => handle_tp_command(*x, *y, *z, context),
    }
}

fn handle_health_command(amount: f32, context: &mut CommandContext<'_>) {
    context.health.set_for_alive(amount);
}

fn handle_upgrade_command(context: &mut CommandContext<'_>) {
    // Upgrade level by 1
    context.level.value += 1;

    // Create NBT for enchantment protection level 1
    let mut protection_nbt = nbt::Compound::new();
    let mut enchantments = vec![];

    let mut protection_enchantment = nbt::Compound::new();
    protection_enchantment.insert("id", nbt::Value::String("minecraft:protection".into()));
    protection_enchantment.insert("lvl", nbt::Value::Short(1));
    enchantments.push(protection_enchantment);
    protection_nbt.insert(
        "Enchantments",
        nbt::Value::List(nbt::list::List::Compound(enchantments)),
    );

    // Apply upgrades based on the level
    match context.level.value {
        1 => context
            .inventory
            .set_hand_slot(0, ItemStack::new(ItemKind::WoodenSword, 1, None)),
        2 => context
            .inventory
            .set_boots(ItemStack::new(ItemKind::LeatherBoots, 1, None)),
        3 => context
            .inventory
            .set_leggings(ItemStack::new(ItemKind::LeatherLeggings, 1, None)),
        4 => context
            .inventory
            .set_chestplate(ItemStack::new(ItemKind::LeatherChestplate, 1, None)),
        5 => context
            .inventory
            .set_helmet(ItemStack::new(ItemKind::LeatherHelmet, 1, None)),
        6 => context
            .inventory
            .set_hand_slot(0, ItemStack::new(ItemKind::StoneSword, 1, None)),
        7 => context
            .inventory
            .set_boots(ItemStack::new(ItemKind::ChainmailBoots, 1, None)),
        8 => context
            .inventory
            .set_leggings(ItemStack::new(ItemKind::ChainmailLeggings, 1, None)),
        9 => {
            context.inventory.set_chestplate(ItemStack::new(
                ItemKind::ChainmailChestplate,
                1,
                None,
            ));
        }
        10 => context
            .inventory
            .set_helmet(ItemStack::new(ItemKind::ChainmailHelmet, 1, None)),
        11 => context
            .inventory
            .set_hand_slot(0, ItemStack::new(ItemKind::IronSword, 1, None)),
        12 => context
            .inventory
            .set_boots(ItemStack::new(ItemKind::IronBoots, 1, None)),
        13 => context
            .inventory
            .set_leggings(ItemStack::new(ItemKind::IronLeggings, 1, None)),
        14 => context
            .inventory
            .set_chestplate(ItemStack::new(ItemKind::IronChestplate, 1, None)),
        15 => context
            .inventory
            .set_helmet(ItemStack::new(ItemKind::IronHelmet, 1, None)),
        16 => context
            .inventory
            .set_hand_slot(0, ItemStack::new(ItemKind::DiamondSword, 1, None)),
        17 => context
            .inventory
            .set_boots(ItemStack::new(ItemKind::DiamondBoots, 1, None)),
        18 => context
            .inventory
            .set_leggings(ItemStack::new(ItemKind::DiamondLeggings, 1, None)),
        19 => {
            context
                .inventory
                .set_chestplate(ItemStack::new(ItemKind::DiamondChestplate, 1, None));
        }
        20 => context
            .inventory
            .set_helmet(ItemStack::new(ItemKind::DiamondHelmet, 1, None)),
        21 => context
            .inventory
            .set_hand_slot(0, ItemStack::new(ItemKind::NetheriteSword, 1, None)),
        22 => context
            .inventory
            .set_boots(ItemStack::new(ItemKind::NetheriteBoots, 1, None)),
        23 => context
            .inventory
            .set_leggings(ItemStack::new(ItemKind::NetheriteLeggings, 1, None)),
        24 => {
            context.inventory.set_chestplate(ItemStack::new(
                ItemKind::NetheriteChestplate,
                1,
                None,
            ));
        }
        25 => context
            .inventory
            .set_helmet(ItemStack::new(ItemKind::NetheriteHelmet, 1, None)),
        26 => {
            // Reset armor and start again with Protection I
            context.inventory.set_boots(ItemStack::new(
                ItemKind::LeatherBoots,
                1,
                Some(protection_nbt.clone()),
            ));
            context.inventory.set_leggings(ItemStack::new(
                ItemKind::LeatherLeggings,
                1,
                Some(protection_nbt.clone()),
            ));
            context.inventory.set_chestplate(ItemStack::new(
                ItemKind::LeatherChestplate,
                1,
                Some(protection_nbt.clone()),
            ));
            context.inventory.set_helmet(ItemStack::new(
                ItemKind::LeatherHelmet,
                1,
                Some(protection_nbt.clone()),
            ));
        }
        _ => {
            // Continue upgrading with Protection I after reset
            let level = (context.level.value - 26) % 24;
            match level {
                1 => context.inventory.set_boots(ItemStack::new(
                    ItemKind::ChainmailBoots,
                    1,
                    Some(protection_nbt.clone()),
                )),
                2 => context.inventory.set_leggings(ItemStack::new(
                    ItemKind::ChainmailLeggings,
                    1,
                    Some(protection_nbt.clone()),
                )),
                3 => context.inventory.set_chestplate(ItemStack::new(
                    ItemKind::ChainmailChestplate,
                    1,
                    Some(protection_nbt.clone()),
                )),
                4 => context.inventory.set_helmet(ItemStack::new(
                    ItemKind::ChainmailHelmet,
                    1,
                    Some(protection_nbt.clone()),
                )),
                5 => context.inventory.set_boots(ItemStack::new(
                    ItemKind::IronBoots,
                    1,
                    Some(protection_nbt.clone()),
                )),
                6 => context.inventory.set_leggings(ItemStack::new(
                    ItemKind::IronLeggings,
                    1,
                    Some(protection_nbt.clone()),
                )),
                7 => context.inventory.set_chestplate(ItemStack::new(
                    ItemKind::IronChestplate,
                    1,
                    Some(protection_nbt.clone()),
                )),
                8 => context.inventory.set_helmet(ItemStack::new(
                    ItemKind::IronHelmet,
                    1,
                    Some(protection_nbt.clone()),
                )),
                9 => context.inventory.set_boots(ItemStack::new(
                    ItemKind::DiamondBoots,
                    1,
                    Some(protection_nbt.clone()),
                )),
                10 => context.inventory.set_leggings(ItemStack::new(
                    ItemKind::DiamondLeggings,
                    1,
                    Some(protection_nbt.clone()),
                )),
                11 => context.inventory.set_chestplate(ItemStack::new(
                    ItemKind::DiamondChestplate,
                    1,
                    Some(protection_nbt.clone()),
                )),
                12 => context.inventory.set_helmet(ItemStack::new(
                    ItemKind::DiamondHelmet,
                    1,
                    Some(protection_nbt.clone()),
                )),
                13 => context.inventory.set_boots(ItemStack::new(
                    ItemKind::NetheriteBoots,
                    1,
                    Some(protection_nbt.clone()),
                )),
                14 => context.inventory.set_leggings(ItemStack::new(
                    ItemKind::NetheriteLeggings,
                    1,
                    Some(protection_nbt.clone()),
                )),
                15 => context.inventory.set_chestplate(ItemStack::new(
                    ItemKind::NetheriteChestplate,
                    1,
                    Some(protection_nbt.clone()),
                )),
                16 => context.inventory.set_helmet(ItemStack::new(
                    ItemKind::NetheriteHelmet,
                    1,
                    Some(protection_nbt.clone()),
                )),
                _ => {} // No upgrade for other levels
            }
        }
    }
}

fn handle_stats(stat: Stat, amount: f32, context: &CommandContext<'_>) {
    context
        .world
        .entity_from_id(context.entity)
        .get::<&mut CombatStats>(|stats| match stat {
            Stat::Armor => stats.armor = amount,
            Stat::Toughness => stats.armor_toughness = amount,
            Stat::Damage => stats.damage = amount,
            Stat::Protection => stats.protection = amount,
        });
}

fn handle_give_command(context: &mut CommandContext<'_>) {
    let mut blue_wool_nbt = nbt::Compound::new();

    let can_place_on = [
        "minecraft:stone",
        "minecraft:dirt",
        "minecraft:grass_block",
        "minecraft:blue_wool",
    ]
    .into_iter()
    .map(std::convert::Into::into)
    .collect();

    blue_wool_nbt.insert("CanPlaceOn", nbt::List::String(can_place_on));

    context
        .inventory
        .try_add_item(ItemStack::new(ItemKind::BlueWool, 4, Some(blue_wool_nbt)));
}

fn handle_dirt_command(x: i32, y: i32, z: i32, context: &mut CommandContext<'_>) {
    let msg = format!("Setting dirt at {x} {y} {z}");
    let pkt = play::GameMessageS2c {
        chat: msg.into_cow_text(),
        overlay: false,
    };

    context
        .compose
        .unicast(&pkt, context.stream, context.system_id, context.world)
        .unwrap();

    let pos = IVec3::new(x, y, z);
    context.blocks.set_block(pos, BlockState::DIRT).unwrap();
}

fn handle_speed_command(amount: f32, context: &CommandContext<'_>) {
    let msg = format!("Setting speed to {amount}");
    let pkt = play::GameMessageS2c {
        chat: msg.into_cow_text(),
        overlay: false,
    };

    context
        .compose
        .unicast(&pkt, context.stream, context.system_id, context.world)
        .unwrap();

    let pkt = fly_speed_packet(amount);
    context
        .compose
        .unicast(&pkt, context.stream, context.system_id, context.world)
        .unwrap();
}

fn handle_team_command(context: &CommandContext<'_>) {
    let msg = format!("You are now on team {}", context.team);
    let text = play::GameMessageS2c {
        chat: msg.into_cow_text(),
        overlay: false,
    };
    context
        .compose
        .unicast(&text, context.stream, context.system_id, context.world)
        .unwrap();
}

fn handle_zombie_command(context: &CommandContext<'_>) {
    static ZOMBIE_PROPERTY: std::sync::LazyLock<valence_protocol::profile::Property> =
        std::sync::LazyLock::new(|| {
            let skin = include_str!("../zombie_skin.json");
            let json: serde_json::Value = serde_json::from_str(skin).unwrap();

            let value = json["textures"].as_str().unwrap().to_string();
            let signature = json["signature"].as_str().unwrap().to_string();

            valence_protocol::profile::Property {
                name: "textures".to_string(),
                value,
                signature: Some(signature),
            }
        });

    let msg = "Turning to zombie";

    // todo: maybe this should be an event?
    let text = play::GameMessageS2c {
        chat: msg.into_cow_text(),
        overlay: false,
    };
    context
        .compose
        .unicast(&text, context.stream, context.system_id, context.world)
        .unwrap();

    let uuids = &[context.uuid];
    // remove from list
    let pkt = play::PlayerRemoveS2c {
        uuids: Cow::Borrowed(uuids),
    };

    context
        .compose
        .unicast(&pkt, context.stream, context.system_id, context.world)
        .unwrap();

    let zombie = &*ZOMBIE_PROPERTY;
    let property = core::slice::from_ref(zombie);

    let singleton_entry = &[PlayerListEntry {
        player_uuid: context.uuid,
        username: Cow::Borrowed(context.name),
        properties: Cow::Borrowed(property),
        chat_data: None,
        listed: true,
        ping: 20,
        game_mode: GameMode::Adventure,
        display_name: Some(context.name.to_string().into_cow_text()),
    }];

    let pkt = PlayerListS2c {
        actions: PlayerListActions::default().with_add_player(true),
        entries: Cow::Borrowed(singleton_entry),
    };

    context
        .compose
        .unicast(&pkt, context.stream, context.system_id, context.world)
        .unwrap();

    // first we need to respawn the player
    let pkt = play::PlayerRespawnS2c {
        dimension_type_name: ident!("minecraft:overworld").into(),
        dimension_name: ident!("minecraft:overworld").into(),
        hashed_seed: 0,
        game_mode: GameMode::Adventure,
        previous_game_mode: OptGameMode::default(),
        is_debug: false,
        is_flat: false,
        copy_metadata: false,
        last_death_location: None,
        portal_cooldown: VarInt::default(),
    };

    context
        .compose
        .unicast(&pkt, context.stream, context.system_id, context.world)
        .unwrap();
}

fn fly_speed_packet(amount: f32) -> PlayerAbilitiesS2c {
    PlayerAbilitiesS2c {
        flags: PlayerAbilitiesFlags::default()
            .with_allow_flying(true)
            .with_flying(true),
        flying_speed: amount,
        fov_modifier: 0.0,
    }
}

fn handle_tphere_command(context: &CommandContext<'_>) {
    // Get the executor's position

    // Get all players with Position component
    // todo: cache this
    let query = context
        .world
        .query::<(&mut Position, &NetworkStreamRef)>()
        .build();

    let executor_pos = *context.position;

    query.each_entity(|entity, (position, io)| {
        // Skip if it's the executor
        if entity == context.entity {
            return;
        }

        *position = executor_pos;

        // send packet
        let pkt = play::PlayerPositionLookS2c {
            position: executor_pos.as_dvec3(),
            yaw: 0.0,
            pitch: 0.0,
            flags: PlayerPositionLookFlags::default(),

            // todo: not sure this is really needed
            teleport_id: VarInt(fastrand::i32(..)),
        };

        context
            .compose
            .unicast(&pkt, *io, context.system_id, context.world)
            .unwrap();
    });

    // Send confirmation message to executor
    let msg = "All players have been teleported to your location";
    let pkt = play::GameMessageS2c {
        chat: msg.into_cow_text(),
        overlay: false,
    };

    context
        .compose
        .unicast(&pkt, context.stream, context.system_id, context.world)
        .unwrap();
}

fn handle_tp_command(x: f32, y: f32, z: f32, context: &mut CommandContext<'_>) {
    // Update position
    context.position.x = x;
    context.position.y = y;
    context.position.z = z;

    // Send teleport packet to the player
    let pkt = play::PlayerPositionLookS2c {
        position: context.position.as_dvec3(),
        yaw: 0.0,
        pitch: 0.0,
        flags: PlayerPositionLookFlags::default(),
        teleport_id: VarInt(fastrand::i32(..)),
    };

    context
        .compose
        .unicast(&pkt, context.stream, context.system_id, context.world)
        .unwrap();

    // Send confirmation message
    let msg = format!("Teleported to {x} {y} {z}");
    let text_pkt = play::GameMessageS2c {
        chat: msg.into_cow_text(),
        overlay: false,
    };

    context
        .compose
        .unicast(&text_pkt, context.stream, context.system_id, context.world)
        .unwrap();
}

#[derive(Component)]
pub struct CommandModule;

impl Module for CommandModule {
    fn module(world: &World) {
        add_to_tree(world);

        let system_id = SystemId(8);

        system!("handle_infection_custom_messages", world, &mut EventQueue<event::PluginMessage<'static>>($))
            .multi_threaded()
            .each_iter(move |_it: TableIter<'_, false>, _, event_queue| {
                for msg in event_queue.drain() {
                    debug!("msg {msg:?}");
                }
            });

        system!("handle_infection_events_player", world, &Compose($), &mut EventQueue<event::Command>($), &mut Blocks($))
            .multi_threaded()
            .each_iter(move |it: TableIter<'_, false>, _, (compose, event_queue, mc)| {
                let span = trace_span!("handle_infection_events_player");
                let _enter = span.enter();

                let world = it.world();
                for event in event_queue.drain() {
                    let executed = event.raw.as_str();

                    debug!("executed: {executed}");

                    let Ok((_, command)) = parse::command(executed) else {
                        return;
                    };

                    world.entity_from_id(event.by).get::<(
                        &NetworkStreamRef,
                        &mut Team,
                        &Uuid,
                        &InGameName,
                        &mut PlayerInventory,
                        &mut Level,
                        &mut Health,
                        &mut Position,
                    )>(
                        |(stream, team, uuid, name, inventory, level, health, position)| {
                            let mut context = CommandContext {
                                entity: event.by,
                                stream: *stream,
                                team,
                                compose,
                                world: &world,
                                blocks: mc,
                                system_id,
                                uuid: uuid.0,
                                name,
                                inventory,
                                level,
                                health,
                                position,
                            };
                            process_command(&command, &mut context);
                        },
                    );
                }
            });
    }
}
