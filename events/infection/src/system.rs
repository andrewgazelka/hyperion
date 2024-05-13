#![allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]

use bvh::{Data, Point};
use evenio::{
    event::{EventMut, Insert, Remove},
    fetch::{Fetcher, Single},
    query::{Query, With},
};
use glam::I16Vec2;
use server::{
    components::{ChunkLocation, FullEntityPose, Vitals, PLAYER_SPAWN_POSITION},
    evenio::{
        entity::EntityId,
        event::{Receiver, ReceiverMut, Sender},
    },
    event,
    event::{Gametick, Shoved},
    util::player_skin::PlayerSkin,
    valence_server::{
        entity::EntityKind,
        protocol::{
            packets::play::entity_equipment_update_s2c::EquipmentEntry,
            status_effects::StatusEffect,
        },
        BlockPos, ItemKind, ItemStack, Text,
    },
};
use tracing::{instrument, warn};

use crate::{
    components::{Human, HumanLocations, Team, Zombie},
    ToZombie,
};

// makes it easier to test with the same account
#[instrument(skip_all)]
pub fn scramble_player_name(mut r: ReceiverMut<event::PlayerInit, ()>) {
    // 10 alphanumeric name using fastrand

    let mut name = r.event.username.to_string();

    // mutate the name
    if name.len() < 16 {
        // append a random letter
        let c = fastrand::alphabetic();
        name.push(c);
    } else {
        let mut buffer = [0; 1]; // Buffer large enough for any ASCII character
        let c = fastrand::alphabetic();
        let result = c.encode_utf8(&mut buffer);

        name.replace_range(..1, result);
    }

    r.event.username = name.into_boxed_str();
}

#[derive(Query)]
pub struct BvhHuman<'a> {
    id: EntityId,
    location: &'a ChunkLocation,
    _human: With<&'static Human>,
}

impl<'a> Point for BvhHuman<'a> {
    fn point(&self) -> I16Vec2 {
        self.location.0
    }
}

impl<'a> Data for BvhHuman<'a> {
    type Unit = EntityId;

    fn data(&self) -> &[EntityId] {
        core::slice::from_ref(&self.id)
    }
}

pub fn calculate_chunk_level_bvh(
    _: Receiver<Gametick>,
    humans: Fetcher<BvhHuman>,
    mut human_locations: Single<&mut HumanLocations>,
) {
    let humans: Vec<_> = humans.iter().collect();

    let len = humans.len();
    human_locations.bvh = bvh::Bvh::build(humans, len);
}

#[instrument(skip_all, level = "trace")]
pub fn point_close_player(
    _: Receiver<Gametick>,
    human_locations: Single<&HumanLocations>,
    zombies: Fetcher<(&ChunkLocation, EntityId, With<&Zombie>)>,
    poses: Fetcher<&FullEntityPose>,
    mut s: Sender<event::PointCompass>,
) {
    for (location, id, _) in zombies {
        let Some(ids) = human_locations.bvh.get_closest_slice(location.0) else {
            continue;
        };

        if ids.is_empty() {
            continue;
        }

        let random = fastrand::usize(..ids.len());
        let point_to_id = ids[random];

        let Ok(point_to_pose) = poses.get(point_to_id) else {
            continue;
        };

        let point_to = BlockPos::from(point_to_pose.position.as_dvec3());

        s.send(event::PointCompass {
            target: id,
            point_to,
        });
    }
}

#[instrument(skip_all)]
pub fn assign_team_on_join(
    r: ReceiverMut<event::PlayerInit, EntityId>,
    mut s: Sender<(Insert<Team>, Insert<Human>)>,
) {
    let target = r.event.target;
    s.insert(target, Team::Human);
    s.insert(target, Human);
}
const COMPASS: ItemStack = ItemStack::new(ItemKind::Compass, 1, None);
const SWORD: ItemStack = ItemStack::new(ItemKind::IronSword, 1, None);

const HELMET: ItemStack = ItemStack::new(ItemKind::NetheriteHelmet, 1, None);
const CHESTPLATE: ItemStack = ItemStack::new(ItemKind::NetheriteChestplate, 1, None);
const LEGGINGS: ItemStack = ItemStack::new(ItemKind::NetheriteLeggings, 1, None);
const BOOTS: ItemStack = ItemStack::new(ItemKind::NetheriteBoots, 1, None);

#[instrument(skip_all)]
pub fn give_armor_on_join(
    r: ReceiverMut<event::PostPlayerJoinWorld, EntityId>,
    mut s: Sender<event::SetEquipment>,
) {
    const EQUIPMENT: &[EquipmentEntry] = &[
        EquipmentEntry {
            slot: 0,
            item: SWORD,
        },
        EquipmentEntry {
            slot: 2,
            item: BOOTS,
        },
        EquipmentEntry {
            slot: 3,
            item: LEGGINGS,
        },
        EquipmentEntry {
            slot: 4,
            item: CHESTPLATE,
        },
        EquipmentEntry {
            slot: 5,
            item: HELMET,
        },
    ];

    s.send(event::SetEquipment::new(r.event.target, EQUIPMENT));
}

#[allow(clippy::type_complexity, reason = "required")]
pub fn to_zombie(
    r: ReceiverMut<ToZombie, (&mut Team, &mut Vitals)>,
    mut s: Sender<(
        Insert<Team>,
        Insert<Zombie>,
        Remove<Human>,
        event::DisguisePlayer,
        event::Teleport,
        event::SetPlayerSkin,
        event::DisplayPotionEffect,
        event::SpeedEffect,
        event::SetEquipment,
    )>,
) {
    // only give compass
    const EQUIPMENT: &[EquipmentEntry] = &[EquipmentEntry {
        slot: 0,
        item: COMPASS,
    }];

    let (team, vitals) = r.query;
    let target = r.event.target;

    *team = Team::Zombie;

    s.send(event::DisguisePlayer {
        target,
        mob: EntityKind::ZOMBIE,
    });

    *vitals = Vitals::ALIVE;

    let zombie_skin = include_bytes!("zombie_skin.json");
    let zombie_skin: PlayerSkin = serde_json::from_slice(zombie_skin).unwrap();

    s.insert(target, Zombie);
    s.remove::<Human>(target);

    s.send(event::SetPlayerSkin {
        target,
        skin: zombie_skin,
    });

    // teleport
    let position = PLAYER_SPAWN_POSITION;
    s.send(event::Teleport { target, position });

    s.send(event::DisguisePlayer {
        target,
        mob: EntityKind::ZOMBIE,
    });

    s.send(event::DisplayPotionEffect {
        target,
        effect: StatusEffect::Speed,
        amplifier: 0, // speed 3
        duration: 99999,
        ambient: false,
        show_particles: true,
        show_icon: true,
    });

    // speed 2
    s.send(event::SpeedEffect::new(target, 0));

    s.send(event::SetEquipment::new(target, EQUIPMENT));
}

#[instrument(skip_all)]
pub fn respawn_on_death(r: Receiver<event::Death, EntityId>, mut s: Sender<ToZombie>) {
    let target = r.event.target;
    s.send(ToZombie { target });
}

#[instrument(skip_all)]
pub fn zombie_command(
    r: ReceiverMut<event::Command, (EntityId, &mut Team)>,
    mut s: Sender<(event::ChatMessage, ToZombie)>,
) {
    // todo: permissions
    let raw = &r.event.raw;

    // todo: how to do commands in non O(n) time?
    if raw != "zombie" {
        return;
    }

    let (target, team) = r.query;

    *team = Team::Zombie;

    s.send(event::ChatMessage {
        target,
        message: Text::text("Turning into zombie"),
    });

    s.send(ToZombie { target });
}

#[instrument(skip_all)]
pub fn bump_into_player(r: ReceiverMut<Shoved, &Team>, fetcher: Fetcher<&Team>) {
    let event = r.event;
    let Ok(&origin_team) = fetcher.get(event.from) else {
        warn!("Shoved event where origin is not on a team");
        return;
    };

    let team = *r.query;

    // if a zombies bumps into a human, they are hurt
    if (origin_team, team) == (Team::Zombie, Team::Human) {
        return;
    }

    // else we are ignoring the bump
    EventMut::take(event);
}

#[instrument(skip_all)]
pub fn disable_attack_team(
    event: ReceiverMut<event::AttackEntity, &Team>,
    fetcher: Fetcher<&Team>,
) {
    let predator_team = event.query;

    let Ok(prey_team) = fetcher.get(event.event.from) else {
        warn!("AttackEntity event where attacker is not on a team");
        return;
    };

    if predator_team == prey_team {
        EventMut::take(event.event);
    }
}
