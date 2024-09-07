use flecs_ecs::{core::EntityView, macros::Component};

#[derive(Component)]
#[repr(C)]
pub enum Team {
    Zombie,
    Player,
}

fn add_zombie(entity: &EntityView) {
    entity.add_enum(Team::Zombie);
}

fn add_player(entity: &EntityView) {
    entity.add_enum(Team::Player);
}
