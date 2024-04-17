use evenio::{entity::EntityId, event::Event};
use glam::Vec3;
use valence_protocol::Hand;

use crate::components::FullEntityPose;

/// Initialize a Minecraft entity (like a zombie) with a given pose.
#[derive(Event)]
pub struct InitEntity {
    /// The pose of the entity.
    pub pose: FullEntityPose,
}

#[derive(Event)]
pub struct InitPlayer {
    #[event(target)]
    pub entity: EntityId,

    /// The name of the player i.e., `Emerald_Explorer`.
    pub name: Box<str>,
    pub uuid: uuid::Uuid,
    pub pose: FullEntityPose,
}

/// Sent whenever a player joins the server.
#[derive(Event)]
pub struct PlayerJoinWorld {
    /// The [`EntityId`] of the player.
    #[event(target)]
    pub target: EntityId,
}

/// An event that is sent whenever a player is kicked from the server.
#[derive(Event)]
pub struct KickPlayer {
    /// The [`EntityId`] of the player.
    #[event(target)] // Works on tuple struct fields as well.
    pub target: EntityId,
    /// The reason the player was kicked.
    pub reason: String,
}

/// An event that is sent whenever a player swings an arm.
#[derive(Event)]
pub struct SwingArm {
    /// The [`EntityId`] of the player.
    #[event(target)]
    pub target: EntityId,
    /// The hand the player is swinging.
    pub hand: Hand,
}

#[derive(Event)]
pub struct AttackEntity {
    /// The [`EntityId`] of the player.
    #[event(target)]
    pub target: EntityId,
    /// The location of the player that is hitting.
    pub from_pos: Vec3,
}

/// An event to kill all minecraft entities (like zombies, skeletons, etc). This will be sent to the equivalent of
/// `/killall` in the game.
#[derive(Event)]
pub struct KillAllEntities;

/// An event when server stats are updated.
#[derive(Event, Copy, Clone)]
pub struct StatsEvent {
    /// The number of milliseconds per tick in the last second.
    pub ms_per_tick_mean_1s: f64,
    /// The number of milliseconds per tick in the last 5 seconds.
    pub ms_per_tick_mean_5s: f64,
}

#[derive(Event)]
pub struct Gametick;

/// An event that is sent when it is time to send packets to clients.
#[derive(Event)]
pub struct Egress;
