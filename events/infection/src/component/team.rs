use std::fmt::Display;

use flecs_ecs::macros::Component;

#[derive(Component, Debug)]
#[repr(C)]
pub enum Team {
    Zombie,
    Player,
}

impl Display for Team {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // https://modrinth.com/resourcepack/565+-minecraft-emoji
        match self {
            Self::Zombie => write!(f, "\u{E050}"),
            Self::Player => write!(f, "\u{E252}"),
        }
    }
}
