use std::borrow::Cow;

use compact_str::format_compact;
use flecs_ecs::{
    core::{
        EntityViewGet, QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World,
        WorldProvider, flecs,
    },
    macros::{Component, system},
    prelude::Module,
};
use hyperion::{
    net::{
        Compose, NetworkStreamRef, agnostic,
        packets::{BossBarAction, BossBarS2c},
    },
    simulation::{EntityReaction, Health, PacketState, Player, Position, event},
    storage::EventQueue,
    system_registry::SystemId,
    util::TracingExt,
    uuid::Uuid,
    valence_protocol::{
        ItemKind, ItemStack, Particle, VarInt, ident,
        math::{DVec3, Vec3},
        nbt,
        packets::{
            play,
            play::{
                boss_bar_s2c::{BossBarColor, BossBarDivision, BossBarFlags},
                entity_attributes_s2c::AttributeProperty,
            },
        },
    },
};
use hyperion_inventory::PlayerInventory;
use hyperion_utils::EntityExt;
use tracing::info_span;

#[derive(Component)]
pub struct AttackModule;

#[derive(Component, Default, Copy, Clone, Debug)]
#[meta]
pub struct ImmuneUntil {
    tick: i64,
}

#[derive(Component, Default, Copy, Clone, Debug)]
#[meta]
pub struct Armor {
    pub armor: f32,
}

// Used as a component only for commands, does not include armor or weapons
#[derive(Component, Default, Copy, Clone, Debug)]
#[meta]
pub struct CombatStats {
    pub armor: f32,
    pub armor_toughness: f32,
    pub damage: f32,
    pub protection: f32,
}

#[derive(Component, Default, Copy, Clone, Debug)]
#[meta]
pub struct KillCount {
    pub kill_count: u32,
}

impl Module for AttackModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        world.component::<ImmuneUntil>().meta();
        world.component::<Armor>().meta();
        world.component::<CombatStats>().meta();
        world.component::<KillCount>().meta();

        world
            .component::<Player>()
            .add_trait::<(flecs::With, ImmuneUntil)>()
            .add_trait::<(flecs::With, CombatStats)>()
            .add_trait::<(flecs::With, KillCount)>()
            .add_trait::<(flecs::With, Armor)>();

        let kill_count_uuid = Uuid::new_v4();

        system!(
            "kill_counts",
            world,
            &Compose($),
            &KillCount,
            &NetworkStreamRef,
        )
        .with_enum(PacketState::Play)
        .multi_threaded()
        .kind::<flecs::pipeline::OnUpdate>()
        .tracing_each_entity(
            info_span!("kill_counts"),
            move |entity, (compose, kill_count, stream)| {
                const MAX_KILLS: usize = 10;

                let world = entity.world();

                let kills = kill_count.kill_count;
                let title = format_compact!("{kills} kills");
                let title = hyperion_text::Text::new(&title);
                let health = (kill_count.kill_count as f32 / MAX_KILLS as f32).min(1.0);

                let pkt = BossBarS2c {
                    id: kill_count_uuid,
                    action: BossBarAction::Add {
                        title,
                        health,
                        color: BossBarColor::Red,
                        division: BossBarDivision::NoDivision,
                        flags: BossBarFlags::default(),
                    },
                };

                compose
                    .unicast(&pkt, *stream, SystemId(99), &world)
                    .unwrap();
            },
        );

        system!("handle_attacks", world, &mut EventQueue<event::AttackEntity>($), &Compose($))
            .multi_threaded()
            .each_iter(
                move |it: TableIter<'_, false>,
                      _,
                      (event_queue, compose): (
                          &mut EventQueue<event::AttackEntity>,
                          &Compose,
                      )| {
                    const IMMUNE_TICK_DURATION: i64 = 10;

                    let span = info_span!("handle_attacks");
                    let _enter = span.enter();

                    let current_tick = compose.global().tick;

                    let world = it.world();

                    for event in event_queue.drain() {
                        let target = world.entity_from_id(event.target);
                        let origin = world.entity_from_id(event.origin);
                        origin.get::<(&Position, &mut KillCount, &mut PlayerInventory, &mut Armor, &CombatStats, &PlayerInventory)>(|(origin_pos, kill_count, inventory, origin_armor, from_stats, from_inventory)| {
                            let damage = from_stats.damage + calculate_stats(from_inventory).damage;
                            target.get::<(
                                &mut ImmuneUntil,
                                &mut Health,
                                &mut Position,
                                &mut EntityReaction,
                                &CombatStats,
                                &PlayerInventory
                            )>(
                                |(immune_until, health, target_position, reaction, stats, target_inventory)| {
                                    if immune_until.tick > current_tick {
                                        return;
                                    }

                                    immune_until.tick = current_tick + IMMUNE_TICK_DURATION;

                                    let calculated_stats = calculate_stats(target_inventory);
                                    let armor = stats.armor + calculated_stats.armor;
                                    let toughness = stats.armor_toughness + calculated_stats.armor_toughness;
                                    let protection = stats.protection + calculated_stats.protection;

                                    let damage_after_armor = get_damage_left(damage, armor, toughness);
                                    let damage_after_protection = get_inflicted_damage(damage_after_armor, protection);

                                    health.damage(damage_after_protection);
                                    if health.is_dead() {
                                        let sound = agnostic::sound(
                                            ident!("minecraft:entity.player.attack.knockback"),
                                            **target_position,
                                        ).volume(1.5)
                                            .pitch(0.8)
                                            .seed(fastrand::i64(..))
                                            .build();

                                        compose.broadcast(&sound, SystemId(999)).send(&world).unwrap();

                                        // Create particle effect at the attacker's position
                                        let particle_pkt = play::ParticleS2c {
                                            particle: Cow::Owned(Particle::Explosion),
                                            long_distance: true,
                                            position: target_position.as_dvec3() + DVec3::new(0.0, 1.0, 0.0),
                                            max_speed: 0.5,
                                            count: 100,
                                            offset: Vec3::new(0.5, 0.5, 0.5),
                                        };

                                        // Add a second particle effect for more visual impact
                                        let particle_pkt2 = play::ParticleS2c {
                                            particle: Cow::Owned(Particle::DragonBreath),
                                            long_distance: true,
                                            position: target_position.as_dvec3() + DVec3::new(0.0, 1.5, 0.0),
                                            max_speed: 0.2,
                                            count: 75,
                                            offset: Vec3::new(0.3, 0.3, 0.3),
                                        };
                                        let origin_entity_id = origin.minecraft_id();

                                        origin_armor.armor += 1.0;
                                        let pkt = play::EntityAttributesS2c {
                                            entity_id: VarInt(origin_entity_id),
                                            properties: vec![
                                                AttributeProperty {
                                                    key: ident!("minecraft:generic.armor").into(),
                                                    value: origin_armor.armor.into(),
                                                    modifiers: vec![],
                                                }
                                            ],
                                        };

                                        compose.broadcast(&pkt, SystemId(999)).send(&world).unwrap();
                                        compose.broadcast(&particle_pkt, SystemId(999)).send(&world).unwrap();
                                        compose.broadcast(&particle_pkt2, SystemId(999)).send(&world).unwrap();

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
                                        match kill_count.kill_count {
                                            0 => {}
                                            1 => inventory
                                                .set_hotbar(0, ItemStack::new(ItemKind::WoodenSword, 1, None)),
                                            2 => inventory
                                                .set_boots(ItemStack::new(ItemKind::LeatherBoots, 1, None)),
                                            3 => inventory
                                                .set_leggings(ItemStack::new(ItemKind::LeatherLeggings, 1, None)),
                                            4 => inventory
                                                .set_chestplate(ItemStack::new(ItemKind::LeatherChestplate, 1, None)),
                                            5 => inventory
                                                .set_helmet(ItemStack::new(ItemKind::LeatherHelmet, 1, None)),
                                            6 => inventory
                                                .set_hotbar(0, ItemStack::new(ItemKind::StoneSword, 1, None)),
                                            7 => inventory
                                                .set_boots(ItemStack::new(ItemKind::ChainmailBoots, 1, None)),
                                            8 => inventory
                                                .set_leggings(ItemStack::new(ItemKind::ChainmailLeggings, 1, None)),
                                            9 => inventory
                                                .set_chestplate(ItemStack::new(ItemKind::ChainmailChestplate, 1, None)),
                                            10 => inventory
                                                .set_helmet(ItemStack::new(ItemKind::ChainmailHelmet, 1, None)),
                                            11 => inventory
                                                .set_hotbar(0, ItemStack::new(ItemKind::IronSword, 1, None)),
                                            12 => inventory
                                                .set_boots(ItemStack::new(ItemKind::IronBoots, 1, None)),
                                            13 => inventory
                                                .set_leggings(ItemStack::new(ItemKind::IronLeggings, 1, None)),
                                            14 => inventory
                                                .set_chestplate(ItemStack::new(ItemKind::IronChestplate, 1, None)),
                                            15 => inventory
                                                .set_helmet(ItemStack::new(ItemKind::IronHelmet, 1, None)),
                                            16 => inventory
                                                .set_hotbar(0, ItemStack::new(ItemKind::DiamondSword, 1, None)),
                                            17 => inventory
                                                .set_boots(ItemStack::new(ItemKind::DiamondBoots, 1, None)),
                                            18 => inventory
                                                .set_leggings(ItemStack::new(ItemKind::DiamondLeggings, 1, None)),
                                            19 => inventory
                                                .set_chestplate(ItemStack::new(ItemKind::DiamondChestplate, 1, None)),
                                            20 => inventory
                                                .set_helmet(ItemStack::new(ItemKind::DiamondHelmet, 1, None)),
                                            21 => inventory
                                                .set_hotbar(0, ItemStack::new(ItemKind::NetheriteSword, 1, None)),
                                            22 => inventory
                                                .set_boots(ItemStack::new(ItemKind::NetheriteBoots, 1, None)),
                                            23 => inventory
                                                .set_leggings(ItemStack::new(ItemKind::NetheriteLeggings, 1, None)),
                                            24 => inventory
                                                .set_chestplate(ItemStack::new(ItemKind::NetheriteChestplate, 1, None)),
                                            25 => inventory
                                                .set_helmet(ItemStack::new(ItemKind::NetheriteHelmet, 1, None)),
                                            26 => {
                                                // Reset armor and start again with Protection I
                                                inventory.set_boots(ItemStack::new(
                                                    ItemKind::LeatherBoots,
                                                    1,
                                                    Some(protection_nbt.clone()),
                                                ));
                                                inventory.set_leggings(ItemStack::new(
                                                    ItemKind::LeatherLeggings,
                                                    1,
                                                    Some(protection_nbt.clone()),
                                                ));
                                                inventory.set_chestplate(ItemStack::new(
                                                    ItemKind::LeatherChestplate,
                                                    1,
                                                    Some(protection_nbt.clone()),
                                                ));
                                                inventory.set_helmet(ItemStack::new(
                                                    ItemKind::LeatherHelmet,
                                                    1,
                                                    Some(protection_nbt.clone()),
                                                ));
                                            }
                                            _ => {
                                                // Continue upgrading with Protection I after reset
                                                let level = (kill_count.kill_count - 26) % 24;
                                                match level {
                                                    1 => inventory.set_boots(ItemStack::new(
                                                        ItemKind::ChainmailBoots,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    2 => inventory.set_leggings(ItemStack::new(
                                                        ItemKind::ChainmailLeggings,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    3 => inventory.set_chestplate(ItemStack::new(
                                                        ItemKind::ChainmailChestplate,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    4 => inventory.set_helmet(ItemStack::new(
                                                        ItemKind::ChainmailHelmet,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    5 => inventory.set_boots(ItemStack::new(
                                                        ItemKind::IronBoots,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    6 => inventory.set_leggings(ItemStack::new(
                                                        ItemKind::IronLeggings,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    7 => inventory.set_chestplate(ItemStack::new(
                                                        ItemKind::IronChestplate,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    8 => inventory.set_helmet(ItemStack::new(
                                                        ItemKind::IronHelmet,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    9 => inventory.set_boots(ItemStack::new(
                                                        ItemKind::DiamondBoots,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    10 => inventory.set_leggings(ItemStack::new(
                                                        ItemKind::DiamondLeggings,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    11 => inventory.set_chestplate(ItemStack::new(
                                                        ItemKind::DiamondChestplate,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    12 => inventory.set_helmet(ItemStack::new(
                                                        ItemKind::DiamondHelmet,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    13 => inventory.set_boots(ItemStack::new(
                                                        ItemKind::NetheriteBoots,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    14 => inventory.set_leggings(ItemStack::new(
                                                        ItemKind::NetheriteLeggings,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    15 => inventory.set_chestplate(ItemStack::new(
                                                        ItemKind::NetheriteChestplate,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    16 => inventory.set_helmet(ItemStack::new(
                                                        ItemKind::NetheriteHelmet,
                                                        1,
                                                        Some(protection_nbt.clone()),
                                                    )),
                                                    _ => {} // No upgrade for other levels
                                                }
                                            }
                                        }
                                        // player died, increment kill count
                                        kill_count.kill_count += 1;
                                        return;
                                    }

                                    // Calculate velocity change based on attack direction
                                    let this = **target_position;
                                    let other = **origin_pos;

                                    let delta_x = other.x - this.x;
                                    let delta_z = other.z - this.z;

                                    if delta_x.abs() >= 0.01 || delta_z.abs() >= 0.01 {
                                        let dist_xz = delta_x.hypot(delta_z);
                                        let multiplier = 0.4;

                                        reaction.velocity /= 2.0;
                                        reaction.velocity.x -= delta_x / dist_xz * multiplier;
                                        reaction.velocity.y += multiplier;
                                        reaction.velocity.z -= delta_z / dist_xz * multiplier;

                                        reaction.velocity.y = reaction.velocity.y.min(0.4);
                                    }
                                },
                            );
                        });
                    }
                },
            );
    }
}

// From minecraft source
fn get_damage_left(damage: f32, armor: f32, armor_toughness: f32) -> f32 {
    let f: f32 = 2.0 + armor_toughness / 4.0;
    let g: f32 = (armor - damage / f).clamp(armor * 0.2, 20.0);
    damage * (1.0 - g / 25.0)
}

fn get_inflicted_damage(damage: f32, protection: f32) -> f32 {
    let f: f32 = protection.clamp(0.0, 20.0);
    damage * (1.0 - f / 25.0)
}

const fn calculate_damage(item: &ItemStack) -> f32 {
    match item.item {
        ItemKind::WoodenSword | ItemKind::GoldenSword => 4.0,
        ItemKind::StoneSword => 5.0,
        ItemKind::IronSword => 6.0,
        ItemKind::DiamondSword => 7.0,
        ItemKind::NetheriteSword => 8.0,
        _ => 1.0,
    }
}

const fn calculate_armor(item: &ItemStack) -> f32 {
    match item.item {
        ItemKind::LeatherHelmet
        | ItemKind::LeatherBoots
        | ItemKind::GoldenHelmet
        | ItemKind::GoldenBoots
        | ItemKind::ChainmailHelmet
        | ItemKind::ChainmailBoots => 1.0,
        ItemKind::LeatherLeggings
        | ItemKind::GoldenLeggings
        | ItemKind::IronHelmet
        | ItemKind::IronBoots => 2.0,
        ItemKind::LeatherChestplate
        | ItemKind::DiamondHelmet
        | ItemKind::DiamondBoots
        | ItemKind::NetheriteHelmet
        | ItemKind::NetheriteBoots => 3.0,
        ItemKind::ChainmailLeggings => 4.0,
        ItemKind::IronLeggings | ItemKind::GoldenChestplate | ItemKind::ChainmailChestplate => 5.0,
        ItemKind::IronChestplate | ItemKind::DiamondLeggings | ItemKind::NetheriteLeggings => 6.0,
        ItemKind::DiamondChestplate | ItemKind::NetheriteChestplate => 8.0,
        _ => 0.0,
    }
}

const fn calculate_toughness(item: &ItemStack) -> f32 {
    match item.item {
        ItemKind::DiamondHelmet
        | ItemKind::DiamondChestplate
        | ItemKind::DiamondLeggings
        | ItemKind::DiamondBoots => 2.0,

        ItemKind::NetheriteHelmet
        | ItemKind::NetheriteChestplate
        | ItemKind::NetheriteLeggings
        | ItemKind::NetheriteBoots => 3.0,
        _ => 0.0,
    }
}

fn calculate_stats(inventory: &PlayerInventory) -> CombatStats {
    let hand = inventory.get_hand_slot(0).unwrap();
    let damage = calculate_damage(hand);
    let armor = calculate_armor(inventory.get_helmet())
        + calculate_armor(inventory.get_chestplate())
        + calculate_armor(inventory.get_leggings())
        + calculate_armor(inventory.get_boots());

    let armor_toughness = calculate_toughness(inventory.get_helmet())
        + calculate_toughness(inventory.get_chestplate())
        + calculate_toughness(inventory.get_leggings())
        + calculate_toughness(inventory.get_boots());

    CombatStats {
        armor,
        armor_toughness,
        damage,
        // TODO
        protection: 0.0,
    }
}
