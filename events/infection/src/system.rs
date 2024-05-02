#![allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]

use evenio::{
    event::{EventMut, Insert},
    fetch::Fetcher,
};
use server::{
    components::{Vitals, PLAYER_SPAWN_POSITION},
    evenio::{
        entity::EntityId,
        event::{Receiver, ReceiverMut, Sender},
    },
    event,
    event::Shoved,
    util::player_skin::PlayerSkin,
    valence_server::{entity::EntityKind, Text},
};
use tracing::{instrument, warn};

use crate::components::Team;

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

#[instrument(skip_all)]
pub fn assign_team_on_join(
    r: ReceiverMut<event::PlayerInit, EntityId>,
    mut s: Sender<Insert<Team>>,
) {
    s.insert(r.event.target, Team::Human);
}

#[instrument(skip_all)]
pub fn respawn_on_death(
    r: Receiver<event::Death, (EntityId, &mut Team, &mut Vitals)>,
    mut s: Sender<(event::DisguisePlayer, event::Teleport)>,
) {
    // if they die they become zombies

    let (target, team, vitals) = r.query;

    *team = Team::Zombie;

    s.send(event::DisguisePlayer {
        target,
        mob: EntityKind::ZOMBIE,
    });

    // teleport
    let position = PLAYER_SPAWN_POSITION;
    s.send(event::Teleport { target, position });
    *vitals = Vitals::ALIVE;
}

#[instrument(skip_all)]
pub fn zombie_command(
    r: ReceiverMut<event::Command, (EntityId, &mut Team)>,
    mut s: Sender<(
        event::DisguisePlayer,
        event::ChatMessage,
        event::SetPlayerSkin,
    )>,
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

    let zombie_skin = include_bytes!("zombie_skin.json");
    let zombie_skin: PlayerSkin = serde_json::from_slice(zombie_skin).unwrap();

    s.send(event::SetPlayerSkin {
        target,
        skin: zombie_skin,
    });

    s.send(event::DisguisePlayer {
        target,
        mob: EntityKind::ZOMBIE,
    });
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
