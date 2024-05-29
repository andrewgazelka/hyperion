#![allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]

use bvh::{Data, Point};
use evenio::{
    event::{EventMut, Insert, Remove},
    fetch::{Fetcher, Single},
    query::{Query, With},
    rayon,
};
use glam::I16Vec2;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use server::{
    components::{chunks::Chunks, ChunkLocation, FullEntityPose, Vitals, PLAYER_SPAWN_POSITION},
    evenio::{
        entity::EntityId,
        event::{Receiver, ReceiverMut, Sender},
    },
    event,
    event::{BulkShoved, Gametick, Shoved},
    util::player_skin::PlayerSkin,
    valence_server::{entity::EntityKind, protocol::status_effects::StatusEffect, BlockPos, Text},
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

#[instrument(skip_all)]
pub fn block_finish_break(
    r: Receiver<event::BlockFinishBreak, ()>,
    chunks: Single<&Chunks>,
    sender: Sender<event::UpdateBlock>,
) {
    let position = r.event.position;

    let block = chunks.get_block(position);

    println!("block finish break {position:?} {block:?}");

    let Some(block) = block else {
        return;
    };

    sender.send(event::UpdateBlock {
        position,
        id: block,
        sequence: r.event.sequence,
    });
}

#[instrument(skip_all)]
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
    s: Sender<event::PointCompass>,
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

        s.send_to(id, event::PointCompass { point_to });
    }
}

#[instrument(skip_all)]
pub fn assign_team_on_join(
    r: ReceiverMut<event::PlayerInit, EntityId>,
    s: Sender<(Insert<Team>, Insert<Human>)>,
) {
    let target = r.query;
    s.insert(target, Team::Human);
    s.insert(target, Human);
}

#[allow(clippy::type_complexity, reason = "required")]
#[instrument(skip_all)]
pub fn to_zombie(
    r: ReceiverMut<ToZombie, (&mut Team, &mut Vitals, EntityId)>,
    s: Sender<(
        Insert<Team>,
        Insert<Zombie>,
        Remove<Human>,
        event::DisguisePlayer,
        event::Teleport,
        event::SetPlayerSkin,
        event::DisplayPotionEffect,
        event::SpeedEffect,
    )>,
) {
    let (team, vitals, target) = r.query;

    *team = Team::Zombie;

    s.send_to(target, event::DisguisePlayer {
        mob: EntityKind::ZOMBIE,
    });

    *vitals = Vitals::ALIVE;

    let zombie_skin = include_bytes!("zombie_skin.json");
    let zombie_skin: PlayerSkin = serde_json::from_slice(zombie_skin).unwrap();

    s.insert(target, Zombie);
    s.remove::<Human>(target);

    s.send_to(target, event::SetPlayerSkin { skin: zombie_skin });

    // teleport
    let position = PLAYER_SPAWN_POSITION;
    s.send_to(target, event::Teleport { position });

    s.send_to(target, event::DisguisePlayer {
        mob: EntityKind::ZOMBIE,
    });

    s.send_to(target, event::DisplayPotionEffect {
        effect: StatusEffect::Speed,
        amplifier: 0, // speed 3
        duration: 99999,
        ambient: false,
        show_particles: true,
        show_icon: true,
    });

    // speed 2
    s.send_to(target, event::SpeedEffect::new(0));
}

#[instrument(skip_all)]
pub fn respawn_on_death(r: Receiver<event::Death, EntityId>, s: Sender<ToZombie>) {
    let target = r.query;
    s.send_to(target, ToZombie);
}

#[instrument(skip_all)]
pub fn zombie_command(
    r: ReceiverMut<event::Command, (EntityId, &mut Team)>,
    s: Sender<(event::ChatMessage, ToZombie)>,
) {
    // todo: permissions
    let raw = &r.event.raw;

    // todo: how to do commands in non O(n) time?
    if raw != "zombie" {
        return;
    }

    let (target, team) = r.query;

    *team = Team::Zombie;

    s.send_to(target, event::ChatMessage {
        message: Text::text("Turning into zombie"),
    });

    s.send_to(target, ToZombie);
}

#[instrument(skip_all)]
pub fn bump_into_player(mut r: ReceiverMut<BulkShoved>, fetcher: Fetcher<&Team>) {
    r.event.0.get_all_mut().par_iter_mut().for_each(|lst| {
        let mut lst = lst.borrow_mut();
        lst.retain(|Shoved { target, from, .. }| {
            let Ok(&origin_team) = fetcher.get(*from) else {
                return false;
            };

            let Ok(&team) = fetcher.get(*target) else {
                return false;
            };

            (origin_team, team) == (Team::Zombie, Team::Human)
        });
    });
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
