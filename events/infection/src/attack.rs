use flecs_ecs::{
    core::{flecs, EntityViewGet, QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    net::{Compose, NetworkStreamRef},
    simulation::{event, metadata::Metadata, EntityReaction, Health, Player, Position},
    storage::EventQueue,
    system_registry::SystemId,
    valence_protocol::{
        ident,
        packets::play,
        sound::{SoundCategory, SoundId},
        ItemKind, ItemStack, VarInt,
    },
};
use hyperion_inventory::{Inventory, PlayerInventory};
use tracing::trace_span;

#[derive(Component)]
pub struct AttackModule;

#[derive(Component, Default, Copy, Clone, Debug)]
pub struct ImmuneUntil {
    tick: i64,
}

#[derive(Component, Default, Copy, Clone, Debug)]
pub struct CombatStats {
    pub armor: f32,
    pub armor_toughness: f32,
    pub damage: f32,
    pub protection: f32,
}

impl Module for AttackModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        world
            .component::<Player>()
            .add_trait::<(flecs::With, ImmuneUntil)>()
            .add_trait::<(flecs::With, CombatStats)>();

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

                    let span = trace_span!("handle_attacks");
                    let _enter = span.enter();

                    let current_tick = compose.global().tick;

                    let world = it.world();

                    for event in event_queue.drain() {
                        let target = world.entity_from_id(event.target);
                        let origin = world.entity_from_id(event.origin);

                        println!("{:?}", event.origin);

                        let (damage, from_pos) = origin.get::<(&CombatStats, &Position, &PlayerInventory)>(|(stats, pos, inventory)| (stats.damage + calculate_stats(inventory).damage, pos.position));

                        target.get::<(
                            &NetworkStreamRef,
                            &mut ImmuneUntil,
                            &mut Health,
                            &mut Metadata,
                            &Position,
                            &mut EntityReaction,
                            &CombatStats,
                            &PlayerInventory
                        )>(
                            |(network_ref, immune_until, health, metadata, position, reaction, stats, inventory)| {
                                if immune_until.tick > current_tick {
                                    return;
                                }

                                immune_until.tick = current_tick + IMMUNE_TICK_DURATION;

                                let calculated_stats = calculate_stats(inventory);
                                let armor = stats.armor + calculated_stats.armor;
                                let toughness = stats.armor_toughness + calculated_stats.armor_toughness;
                                let protection = stats.protection + calculated_stats.protection;

                                let damage_after_armor = get_damage_left(damage, armor, toughness);
                                let damage_after_protection = get_inflicted_damage(damage_after_armor, protection);

                                health.normal -= damage_after_protection;

                                metadata.health(health.normal);

                                let pkt = play::HealthUpdateS2c {
                                    health: health.normal,
                                    food: VarInt(10),
                                    food_saturation: 10.0
                                };

                                compose.unicast(&pkt, *network_ref, SystemId(999), &world).unwrap();

                                let entity_id = VarInt(event.target.0 as i32);

                                let pkt = play::EntityDamageS2c {
                                    entity_id,
                                    source_type_id: Default::default(),
                                    source_cause_id: Default::default(),
                                    source_direct_id: Default::default(),
                                    source_pos: None,
                                };

                                compose.broadcast(&pkt, SystemId(999)).send(&world).unwrap();

                                // let pkt = play::EntityAttributesS2c {
                                //     entity_id,
                                //     properties: vec![
                                //         AttributeProperty {
                                //             key: (),
                                //             value: 0.0,
                                //             modifiers: vec![],
                                //         }
                                //     ],
                                // }

                                // Play a sound when an entity is damaged
                                let ident = ident!("minecraft:entity.player.hurt");
                                let pkt = play::PlaySoundS2c {
                                    id: SoundId::Direct {
                                        id: ident.into(),
                                        range: None,
                                    },
                                    position: (position.position * 8.0).as_ivec3(),
                                    volume: 1.0,
                                    pitch: 1.0,
                                    seed: fastrand::i64(..),
                                    category: SoundCategory::Player,
                                };
                                compose.broadcast(&pkt, SystemId(999)).send(&world).unwrap();

                                // Calculate velocity change based on attack direction
                                let this = position.position;
                                let other = from_pos;

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
                    }
                },
            );
    }
}

// From minecraft source
fn get_damage_left(damage: f32, armor: f32, armor_toughness: f32) -> f32 {
    let f: f32 = 2.0 + armor_toughness / 4.0;
    let g: f32 = (armor - damage / f).clamp(armor * 0.2, 20.0);
    return damage * (1.0 - g / 25.0);
}

fn get_inflicted_damage(damage: f32, protection: f32) -> f32 {
    let f: f32 = protection.clamp(0.0, 20.0);
    return damage * (1.0 - f / 25.0);
}

fn calculate_damage(item: &ItemStack) -> f32 {
    match item.item {
        ItemKind::WoodenSword => 4.0,
        ItemKind::GoldenSword => 4.0,
        ItemKind::StoneSword => 5.0,
        ItemKind::IronSword => 6.0,
        ItemKind::DiamondSword => 7.0,
        ItemKind::NetheriteSword => 8.0,
        _ => 1.0,
    }
}

fn calculate_armor(item: &ItemStack) -> f32 {
    match item.item {
        ItemKind::LeatherHelmet => 1.0,
        ItemKind::LeatherChestplate => 3.0,
        ItemKind::LeatherLeggings => 2.0,
        ItemKind::LeatherBoots => 1.0,

        ItemKind::GoldenHelmet => 2.0,
        ItemKind::GoldenChestplate => 5.0,
        ItemKind::GoldenLeggings => 3.0,
        ItemKind::GoldenBoots => 1.0,

        ItemKind::ChainmailHelmet => 2.0,
        ItemKind::ChainmailChestplate => 5.0,
        ItemKind::ChainmailLeggings => 4.0,
        ItemKind::ChainmailBoots => 1.0,

        ItemKind::IronHelmet => 2.0,
        ItemKind::IronChestplate => 6.0,
        ItemKind::IronLeggings => 5.0,
        ItemKind::IronBoots => 2.0,

        ItemKind::DiamondHelmet => 3.0,
        ItemKind::DiamondChestplate => 8.0,
        ItemKind::DiamondLeggings => 6.0,
        ItemKind::DiamondBoots => 3.0,

        ItemKind::NetheriteHelmet => 3.0,
        ItemKind::NetheriteChestplate => 8.0,
        ItemKind::NetheriteLeggings => 6.0,
        ItemKind::NetheriteBoots => 3.0,
        _ => 0.0,
    }
}

fn calculate_toughness(item: &ItemStack) -> f32 {
    match item.item {
        ItemKind::DiamondHelmet => 2.0,
        ItemKind::DiamondChestplate => 2.0,
        ItemKind::DiamondLeggings => 2.0,
        ItemKind::DiamondBoots => 2.0,

        ItemKind::NetheriteHelmet => 3.0,
        ItemKind::NetheriteChestplate => 3.0,
        ItemKind::NetheriteLeggings => 3.0,
        ItemKind::NetheriteBoots => 3.0,
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
        protection: 0.0,
    }
}
