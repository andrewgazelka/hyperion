use flecs_ecs::core::World;
use hyperion::event::sync::PlayerJoinServer;

use crate::component::team::Team;

pub fn scramble_player_name(_world: &World, event: &mut PlayerJoinServer) {
    // let mut characters: Vec<_> = event.username.chars().collect();
    // fastrand::shuffle(&mut characters);
    //
    // event.username = characters.into_iter().collect();
}

pub fn add_player_to_team(world: &World, event: &mut PlayerJoinServer) {
    world.entity_from_id(event.entity).set(Team::Player);
}
