use std::collections::HashMap;

use bvh_region::{aabb::Aabb, HasAabb};
use derive_more::{Deref, DerefMut, Display, From};
use flecs_ecs::prelude::*;
use glam::{IVec2, IVec3, Vec3};
use serde::{Deserialize, Serialize};
use skin::PlayerSkin;
use uuid;
use valence_protocol::BlockPos;

use crate::Global;

pub mod animation;
pub mod blocks;
pub mod command;
pub mod event;
pub mod handlers;
pub mod metadata;
pub mod skin;
pub mod util;

#[derive(Component, Default, Debug, Deref, DerefMut)]
pub struct StreamLookup {
    /// The UUID of all players
    inner: rustc_hash::FxHashMap<u64, Entity>,
}

#[derive(Component, Default, Debug, Deref, DerefMut)]
pub struct PlayerUuidLookup {
    /// The UUID of all players
    inner: HashMap<Uuid, Entity>,
}

/// The data associated with a player
#[derive(Debug, Copy, Clone)]
pub struct LookupData {
    /// The entity id of the player
    pub id: usize,
    /// The bounding box of the player
    pub aabb: Aabb,
}

impl HasAabb for LookupData {
    fn aabb(&self) -> Aabb {
        self.aabb
    }
}

#[derive(Component, Debug, Default)]
pub struct PlayerBoundingBoxes {
    /// The bounding boxes of all players
    pub query: bvh_region::Bvh<LookupData>,
}

impl PlayerBoundingBoxes {
    /// Get the closest player to the given position.
    #[must_use]
    pub fn closest_to(&self, point: Vec3) -> Option<&LookupData> {
        let (target, _) = self.query.get_closest(point)?;
        Some(target)
    }
}

/// Communicates with the proxy server.
#[derive(Component, Deref, DerefMut, From)]
pub struct EgressComm {
    tx: tokio::sync::mpsc::UnboundedSender<bytes::Bytes>,
}

/// The in-game name of a player.
#[derive(Component, Deref, From, Display, Debug)]
pub struct InGameName(Box<str>);

/// This component is added to all players once they reach the play state. See [`PacketState::Play`].
#[derive(Component, Default)]
pub struct Play;

/// A component that represents a Player. In the future, this should be broken up into multiple components.
///
/// Why should it be broken up? The more things are broken up, the more we can take advantage of Rust borrowing rules.
#[derive(Component, Debug, Default)]
pub struct Player;

/// The state of the login process.
#[derive(Component, Debug, Eq, PartialEq)]
#[repr(C)]
pub enum PacketState {
    Handshake,
    Status,
    Login,
    Play,
    Terminate,
}

/// The health component.
///
/// Health changes can be applied in any order within a tick.
/// Damage and healing are processed as they occur, with the health value
/// being updated immediately after each change.
///
/// Once an entity's health reaches 0, it is considered dead and cannot be
/// revived by healing alone. The entity remains dead until explicitly reset.
///
/// To revive a dead entity, use the [`Health::reset`] method, typically when respawning.
/// This method restores the entity to full health and allows it to be affected
/// by subsequent health changes.
#[derive(Component, Debug, PartialEq)]
pub struct Health {
    value: f32,

    pending: f32,
}

pub const FULL_HEALTH: f32 = 20.0;

impl Health {
    /// Checks if there's a pending health update and returns it.
    ///
    /// This method updates the current health value with the pending value
    /// and returns an [`event::HealthUpdate`] if there was a change.
    pub fn pop_updated(&mut self) -> Option<event::HealthUpdate> {
        #[expect(clippy::float_cmp)]
        let result = (self.pending != self.value).then_some(event::HealthUpdate {
            from: self.value,
            to: self.pending,
        });

        self.value = self.pending;
        result
    }

    /// Returns the current health value.
    #[must_use]
    pub const fn get(&self) -> f32 {
        self.value
    }

    pub fn just_damaged(&mut self) -> bool {
        self.pending < self.value
    }

    /// Checks if the entity is dead (health is 0).
    #[must_use]
    pub const fn is_dead(&self) -> bool {
        self.pending == 0.0
    }

    /// Sets the health of a living entity to a specific value.
    ///
    /// This method will not affect dead entities. To revive a dead entity,
    /// use [`Self::reset`] instead.
    pub fn set_for_alive(&mut self, value: f32) {
        if self.is_dead() {
            return;
        }

        self.pending = value;
    }

    /// Applies damage to the entity, reducing its health.
    ///
    /// The resulting health will be clamped between 0 and [`FULL_HEALTH`].
    pub fn damage(&mut self, amount: f32) {
        self.pending = (self.pending - amount).clamp(0.0, FULL_HEALTH);
    }

    /// Resets the entity's health to full, reviving it if dead.
    ///
    /// This is the only way to revive a dead entity.
    pub fn reset(&mut self) {
        self.pending = FULL_HEALTH;
    }

    /// Heals the entity, increasing its health.
    ///
    /// This method will not affect dead entities. The resulting health
    /// will be clamped between 0 and [`FULL_HEALTH`].
    pub fn heal(&mut self, amount: f32) {
        if self.is_dead() {
            return;
        }

        self.pending = (self.pending + amount).clamp(0.0, FULL_HEALTH);
    }
}

#[derive(Component, Debug, Default, Deref, DerefMut)]
pub struct ConfirmBlockSequences(pub Vec<i32>);

// use unicode hearts
impl Display for Health {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "we want saturating ceiling"
        )]
        let normal = usize::try_from(self.value.ceil() as isize).unwrap_or(0);

        let full_hearts = normal / 2;
        for _ in 0..full_hearts {
            write!(f, "\u{E001}")?;
        }

        if normal % 2 == 1 {
            // half heart
            write!(f, "\u{E002}")?;
        }

        Ok(())
    }
}

impl Default for Health {
    fn default() -> Self {
        Self {
            value: 20.0,
            pending: 20.0,
        }
    }
}

#[derive(Component, Debug, Eq, PartialEq, Default)]
#[expect(missing_docs)]
pub struct ImmuneStatus {
    /// The tick until the player is immune to player attacks.
    pub until: i64,
}

impl ImmuneStatus {
    #[must_use]
    #[expect(missing_docs)]
    pub const fn is_invincible(&self, global: &Global) -> bool {
        global.tick < self.until
    }
}

/// Communication struct. Maybe should be refactored to some extent.
/// This to communicate back from an [`crate::runtime::AsyncRuntime`] to the main thread.
#[derive(Component)]
pub struct Comms {
    /// Skin rx channel.
    pub skins_rx: kanal::Receiver<(Entity, PlayerSkin)>,
    /// Skin tx channel.
    pub skins_tx: kanal::Sender<(Entity, PlayerSkin)>,
}

impl Default for Comms {
    fn default() -> Self {
        let (skins_tx, skins_rx) = kanal::unbounded();

        Self { skins_rx, skins_tx }
    }
}

/// A UUID component. Generally speaking, this tends to be tied to entities with a [`Player`] component.
#[derive(Component, Copy, Clone, Debug, Deref, From, Hash, Eq, PartialEq)]
pub struct Uuid(pub uuid::Uuid);

/// Any living minecraft entity that is NOT a player.
///
/// Example: zombie, skeleton, etc.
#[derive(Component, Debug)]
pub struct Npc;

/// The running multiplier of the entity. This defaults to 1.0.
#[derive(Component, Debug, Copy, Clone)]
pub struct RunningSpeed(pub f32);

impl Default for RunningSpeed {
    fn default() -> Self {
        Self(0.1)
    }
}

/// If the entity can be targeted by non-player entities.
#[derive(Component)]
pub struct AiTargetable;

/// The full pose of an entity. This is used for both [`Player`] and [`Npc`].
#[derive(Component, Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Position {
    /// The (x, y, z) position of the entity.
    /// Note we are using [`Vec3`] instead of [`glam::DVec3`] because *cache locality* is important.
    /// However, the Notchian server uses double precision floating point numbers for the position.
    pub position: Vec3,

    /// The yaw of the entity's head measured in degrees. (todo: probably need a separate component for body yaw, perhaps separate this out)
    pub yaw: f32,

    /// The pitch of the entity's head measured in degrees.
    pub pitch: f32,

    /// The bounding box of the entity.
    pub bounding: Aabb,
}

impl Position {
    #[must_use]
    pub fn sound_position(&self) -> IVec3 {
        let position = self.position * 8.0;
        position.as_ivec3()
    }
}

impl Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let position = self.position;
        let yaw = self.yaw;
        let pitch = self.pitch;
        let bounding = self.bounding;

        write!(f, "@{position}, {yaw}°, {pitch}°, ({bounding})")
    }
}

#[derive(Component, Debug, Copy, Clone)]
#[expect(missing_docs)]
pub struct ChunkPosition(pub IVec2);

const SANE_MAX_RADIUS: i32 = 128;

impl ChunkPosition {
    #[must_use]
    #[expect(missing_docs)]
    pub const fn null() -> Self {
        // todo: huh
        Self(IVec2::new(SANE_MAX_RADIUS, SANE_MAX_RADIUS))
    }
}

/// The initial player spawn position. todo: this should not be a constant
pub const PLAYER_SPAWN_POSITION: Vec3 = Vec3::new(-8_526_209_f32, 100f32, -6_028_464f32);

impl Position {
    // todo: possible have separate field for head yaw
    /// The player's head yaw.
    #[must_use]
    pub const fn head_yaw(&self) -> f32 {
        self.yaw
    }

    /// Create a new [`Position`] for a player given their position and head yaw.
    #[must_use]
    pub fn player(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            bounding: Aabb::create(position, 0.6, 1.8),
        }
    }

    /// Get the start and end block positions of the player's bounding box.
    #[must_use]
    pub fn block_pose_range(&self) -> (BlockPos, BlockPos) {
        let min = self.bounding.min.floor().as_ivec3();
        let max = self.bounding.max.ceil().as_ivec3();

        (
            BlockPos::new(min.x, min.y, min.z),
            BlockPos::new(max.x, max.y, max.z),
        )
    }

    /// Get the chunk position of the center of the player's bounding box.
    #[must_use]
    pub fn chunk_pos(&self) -> IVec2 {
        let position = self.position.as_ivec3();
        let x = position.x >> 4;
        let z = position.z >> 4;
        IVec2::new(x, z)
    }
}

impl Position {
    /// Move the pose by the given vector.
    pub fn move_by(&mut self, vec: Vec3) {
        self.position += vec;
        self.bounding = self.bounding.move_by(vec);
    }

    /// Teleport the pose to the given position.
    pub fn move_to(&mut self, pos: Vec3) {
        self.bounding = self.bounding.move_to_feet(pos);
        self.position = pos;
    }
}

/// The reaction of an entity, in particular to collisions as calculated in `entity_detect_collisions`.
///
/// Why is this useful?
///
/// - We want to be able to detect collisions in parallel.
/// - Since we are accessing bounding boxes in parallel,
///   we need to be able to make sure the bounding boxes are immutable (unless we have something like a
///   [`std::sync::Arc`] or [`std::sync::RwLock`], but this is not efficient).
/// - Therefore, we have an [`EntityReaction`] component which is used to store the reaction of an entity to collisions.
/// - Later we can apply the reaction to the entity's [`Position`] to move the entity.
#[derive(Component, Default, Debug)]
pub struct EntityReaction {
    /// The velocity of the entity.
    pub velocity: Vec3,
}

#[derive(Component)]
pub struct SimModule;

impl Module for SimModule {
    fn module(world: &World) {
        world.component::<Position>();
        world.component::<Player>();
        world.component::<InGameName>();
        world.component::<AiTargetable>();
        world.component::<ImmuneStatus>();
        world.component::<Uuid>();
        world.component::<Health>();
        world.component::<ChunkPosition>();
        world.component::<EntityReaction>();
        world.component::<Play>();
        world.component::<ConfirmBlockSequences>();
        world.component::<metadata::Metadata>();
        world.component::<animation::ActiveAnimation>();

        world.component::<hyperion_inventory::PlayerInventory>();
    }
}
