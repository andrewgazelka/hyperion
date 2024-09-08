use flecs_ecs::macros::Component;

#[derive(Component, Debug)]
#[repr(C)]
pub enum Team {
    Zombie,
    Player,
}
