use flecs_ecs::{core::Entity, macros::Component};
use glam::Vec3;

#[derive(Component, Copy, Clone, Debug, PartialEq)]
pub struct AttackEntity {
    /// The location of the player that is hitting.
    pub from_pos: Vec3,
    pub from: Entity,
    pub damage: f32,
}
