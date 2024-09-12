use flecs_ecs::core::World;
use hyperion::event::sync::PlayerJoinServer;

use crate::component::team::Team;

pub fn add_player_to_team(world: &World, event: &mut PlayerJoinServer) {
    world.entity_from_id(event.entity).set(Team::Player);
}
