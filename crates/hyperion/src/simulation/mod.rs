use std::{borrow::Borrow, collections::HashMap, hash::Hash, sync::Arc};

use bvh_region::{HasAabb, aabb::Aabb};
use derive_more::{Deref, DerefMut, Display, From};
use flecs_ecs::prelude::*;
use glam::{IVec2, IVec3, Vec3};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use skin::PlayerSkin;
use uuid;

use crate::{
    Global, Prev,
    simulation::{command::Command, metadata::Metadata},
    storage::ThreadLocalVec,
};

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
    inner: FxHashMap<u64, Entity>,
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

#[derive(Debug)]
pub struct DeferredMap<K, V> {
    to_add: ThreadLocalVec<(K, V)>,
    to_remove: ThreadLocalVec<K>,
    map: FxHashMap<K, V>,
}

impl<K, V> Default for DeferredMap<K, V> {
    fn default() -> Self {
        Self {
            to_add: ThreadLocalVec::default(),
            to_remove: ThreadLocalVec::default(),
            map: HashMap::default(),
        }
    }
}

impl<K: Eq + Hash, V> DeferredMap<K, V> {
    pub fn insert(&self, key: K, value: V, world: &World) {
        self.to_add.push((key, value), world);
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.get(key)
    }

    pub fn remove(&self, key: K, world: &World) {
        self.to_remove.push(key, world);
    }
}

impl<K: Eq + Hash, V> DeferredMap<K, V> {
    pub fn update(&mut self) {
        for (key, value) in self.to_add.drain() {
            self.map.insert(key, value);
        }

        for key in self.to_remove.drain() {
            self.map.remove(&key);
        }
    }
}

/// The in-game name of a player.
/// todo: fix the meta
#[derive(Component, Deref, From, Display, Debug)]
#[meta]
pub struct InGameName(Arc<str>);

#[derive(Component, Deref, DerefMut, From, Debug, Default)]
pub struct IgnMap(DeferredMap<Arc<str>, Entity>);

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

#[derive(Component, Debug, Deref, DerefMut, PartialEq, PartialOrd, Copy, Clone)]
#[meta]
pub struct Health(f32);

impl Metadata for Health {
    type Type = f32;

    /// <https://wiki.vg/Entity_metadata#:~:text=Float%20(3)-,Health,-1.0>
    const INDEX: u8 = 9;

    fn to_type(self) -> Self::Type {
        self.0
    }
}

impl Health {
    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.0 <= 0.0
    }

    pub fn heal(&mut self, amount: f32) {
        self.0 += amount;
    }

    pub fn damage(&mut self, amount: f32) {
        self.0 -= amount;
    }

    pub fn set_for_alive(&mut self, value: f32) {
        if self.is_dead() {
            return;
        }

        self.0 = value;
    }
}

pub const FULL_HEALTH: f32 = 20.0;

#[derive(Component, Debug, Default, Deref, DerefMut)]
pub struct ConfirmBlockSequences(pub Vec<i32>);

// use unicode hearts
impl Display for Health {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "we want saturating ceiling"
        )]
        let normal = usize::try_from(self.0.ceil() as isize).unwrap_or(0);

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
        Self(20.0)
    }
}

#[derive(Component, Debug, Eq, PartialEq, Default)]
#[expect(missing_docs)]
#[meta]
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
#[derive(
    Component,
    Copy,
    Clone,
    Debug,
    Serialize,
    Deserialize,
    Deref,
    DerefMut,
    From
)]
#[meta]
pub struct Position {
    /// The (x, y, z) position of the entity.
    /// Note we are using [`Vec3`] instead of [`glam::DVec3`] because *cache locality* is important.
    /// However, the Notchian server uses double precision floating point numbers for the position.
    position: Vec3,
}

#[derive(Component, Copy, Clone, Debug, Deref, DerefMut, Default)]
pub struct Yaw {
    yaw: f32,
}

impl Display for Yaw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let yaw = self.yaw;
        write!(f, "{yaw}")
    }
}

impl Display for Pitch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pitch = self.pitch;
        write!(f, "{pitch}")
    }
}

#[derive(Component, Copy, Clone, Debug, Deref, DerefMut, Default)]
pub struct Pitch {
    pitch: f32,
}

const PLAYER_WIDTH: f32 = 0.6;
const PLAYER_HEIGHT: f32 = 1.8;

#[derive(Component, Copy, Clone, Debug)]
#[meta]
pub struct EntitySize {
    pub half_width: f32,
    pub height: f32,
}

impl Display for EntitySize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let half_width = self.half_width;
        let height = self.height;
        write!(f, "{half_width}x{height}")
    }
}

impl Default for EntitySize {
    fn default() -> Self {
        Self {
            half_width: PLAYER_WIDTH / 2.0,
            height: PLAYER_HEIGHT,
        }
    }
}

impl Position {
    #[must_use]
    pub fn sound_position(&self) -> IVec3 {
        let position = self.position * 8.0;
        position.as_ivec3()
    }
}

#[derive(Component, Debug, Copy, Clone)]
#[meta]
pub struct ChunkPosition {
    pub position: IVec2,
}

const SANE_MAX_RADIUS: i32 = 128;

impl ChunkPosition {
    #[must_use]
    #[expect(missing_docs)]
    pub const fn null() -> Self {
        // todo: huh
        Self {
            position: IVec2::new(SANE_MAX_RADIUS, SANE_MAX_RADIUS),
        }
    }
}

#[must_use]
pub fn aabb(position: Vec3, size: EntitySize) -> Aabb {
    let half_width = size.half_width;
    let height = size.height;
    Aabb::new(
        position - Vec3::new(half_width, 0.0, half_width),
        position + Vec3::new(half_width, height, half_width),
    )
}

#[must_use]
pub fn block_bounds(position: Vec3, size: EntitySize) -> (IVec3, IVec3) {
    let bounding = aabb(position, size);
    let min = bounding.min.floor().as_ivec3();
    let max = bounding.max.ceil().as_ivec3();

    (min, max)
}

/// The initial player spawn position. todo: this should not be a constant
pub const PLAYER_SPAWN_POSITION: Vec3 = Vec3::new(-8_526_209_f32, 100f32, -6_028_464f32);

impl Position {
    /// Get the chunk position of the center of the player's bounding box.
    #[must_use]
    pub fn to_chunk(&self) -> IVec2 {
        let position = self.position.as_ivec3();
        let x = position.x >> 4;
        let z = position.z >> 4;
        IVec2::new(x, z)
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
#[meta]
pub struct EntityReaction {
    /// The velocity of the entity.
    pub velocity: Vec3,
}

#[derive(Component)]
pub struct SimModule;

impl Module for SimModule {
    fn module(world: &World) {
        world.component::<Health>().member::<f32>("level");
        world.component::<Prev<Health>>();

        world.component::<PlayerSkin>();
        world.component::<Command>();

        component!(world, EntitySize).opaque_func(meta_ser_stringify_type_display::<EntitySize>);
        component!(world, IVec3 {
            x: i32,
            y: i32,
            z: i32
        });
        component!(world, Vec3 {
            x: f32,
            y: f32,
            z: f32
        });

        component!(world, IgnMap);

        world.component::<Position>().meta();

        world.component::<Player>();

        world.component::<InGameName>();
        component!(world, InGameName).opaque_func(meta_ser_stringify_type_display::<InGameName>);

        world.component::<AiTargetable>();
        world.component::<ImmuneStatus>().meta();
        world.component::<Uuid>();
        world.component::<ChunkPosition>().meta();
        world.component::<EntityReaction>().meta();
        world.component::<ConfirmBlockSequences>();
        world.component::<animation::ActiveAnimation>();

        world.component::<hyperion_inventory::PlayerInventory>();
    }
}
