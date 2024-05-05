use evenio::entity::EntityId;
use server::evenio::component::Component;

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    Human,
    Zombie,
}

#[derive(Component, Default)]
pub struct HumanLocations {
    pub bvh: bvh::Bvh<EntityId>,
}

#[derive(Component)]
pub struct Human;

#[derive(Component)]
pub struct Zombie;
