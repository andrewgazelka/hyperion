use std::{alloc::Allocator, borrow::Cow, cell::RefCell, fmt::Debug};

use bumpalo::Bump;
use derive_more::{Deref, DerefMut};
use evenio::{component::Component, entity::EntityId, event::Event};
use glam::Vec3;
use rayon_local::RayonLocal;
use valence_generated::{block::BlockState, status_effects::StatusEffect};
use valence_protocol::{
    packets::play::entity_equipment_update_s2c::EquipmentEntry, BlockPos, Hand,
};
use valence_server::entity::EntityKind;
use valence_text::Text;

use crate::{
    components::FullEntityPose,
    net::{Server, MAX_PACKET_SIZE},
    system::LookupData,
    util::player_skin::PlayerSkin,
};

/// Initialize a Minecraft entity (like a zombie) with a given pose.
#[derive(Event)]
pub struct InitEntity {
    /// The pose of the entity.
    pub pose: FullEntityPose,
    pub display: EntityKind,
}

#[derive(Event)]
pub struct Command {
    #[event(target)]
    pub by: EntityId,
    pub raw: String,
}

#[derive(Event)]
pub struct PlayerInit {
    #[event(target)]
    pub target: EntityId,

    /// The name of the player i.e., `Emerald_Explorer`.
    pub username: Box<str>,
    pub pose: FullEntityPose,
}

/// Sent whenever a player joins the server.
#[derive(Event)]
pub struct PlayerJoinWorld {
    /// The [`EntityId`] of the player.
    #[event(target)]
    pub target: EntityId,
}

#[derive(Event)]
pub struct PostPlayerJoinWorld {
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Pose {
    Standing = 0,
    FallFlying = 1,
    Sleeping = 2,
    Swimming = 3,
    SpinAttack = 4,
    Sneaking = 5,
    LongJumping = 6,
    Dying = 7,
    Croaking = 8,
    UsingTongue = 9,
    Sitting = 10,
    Roaring = 11,
    Sniffing = 12,
    Emerging = 13,
    Digging = 14,
}

#[derive(Event)]
#[event(immutable)]
pub struct PoseUpdate {
    #[event(target)]
    pub target: EntityId,
    pub state: Pose,
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
pub struct HurtEntity {
    #[event(target)]
    pub target: EntityId,
    pub damage: f32,
}

pub enum AttackType {
    Shove,
    Melee,
}

#[derive(Event)]
pub struct AttackEntity {
    /// The [`EntityId`] of the player.
    #[event(target)]
    pub target: EntityId,
    /// The location of the player that is hitting.
    pub from_pos: Vec3,
    pub from: EntityId,
    pub damage: f32,
    pub source: AttackType,
}

#[derive(Event)]
#[event(immutable)]
pub struct Death {
    #[event(target)]
    pub target: EntityId,
}

/// An event to kill all minecraft entities (like zombies, skeletons, etc). This will be sent to the equivalent of
/// `/killall` in the game.
#[derive(Event)]
pub struct KillAllEntities;

#[derive(Event)]
pub struct Teleport {
    #[event(target)]
    pub target: EntityId,
    pub position: Vec3,
}

/// i.e., when zombies bump into another player
#[derive(Debug)]
pub struct Shoved {
    pub target: EntityId,
    pub from: EntityId,
    pub from_location: Vec3,
}

#[derive(Event, Debug)]
pub struct BulkShoved(pub RayonLocal<Vec<Shoved>>);

/// An event when server stats are updated.
#[derive(Event)]
pub struct Stats {
    pub ms_per_tick: f64,
}

// todo: naming? this seems bad
#[derive(Debug)]
pub struct Scratch<A: Allocator = std::alloc::Global> {
    inner: Vec<u8, A>,
}

impl Scratch {
    #[must_use]
    pub fn new() -> Self {
        let inner = Vec::with_capacity(MAX_PACKET_SIZE);
        Self { inner }
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}

/// Nice for getting a buffer that can be used for intermediate work
///
/// # Safety
/// - every single time [`ScratchBuffer::obtain`] is called, the buffer will be cleared before returning
/// - the buffer has capacity of at least `MAX_PACKET_SIZE`
pub unsafe trait ScratchBuffer: sealed::Sealed + Debug {
    type Allocator: Allocator;
    fn obtain(&mut self) -> &mut Vec<u8, Self::Allocator>;
}

mod sealed {
    pub trait Sealed {}
}

impl<A: Allocator + Debug> sealed::Sealed for Scratch<A> {}

unsafe impl<A: Allocator + Debug> ScratchBuffer for Scratch<A> {
    type Allocator = A;

    fn obtain(&mut self) -> &mut Vec<u8, Self::Allocator> {
        self.inner.clear();
        &mut self.inner
    }
}

impl<A: Allocator> From<A> for Scratch<A> {
    fn from(allocator: A) -> Self {
        Self {
            inner: Vec::with_capacity_in(MAX_PACKET_SIZE, allocator),
        }
    }
}

#[derive(Event)]
pub struct BlockStartBreak {
    #[event(target)]
    pub by: EntityId,
    pub position: BlockPos,
    pub sequence: i32,
}

#[derive(Event)]
pub struct BlockAbortBreak {
    #[event(target)]
    pub by: EntityId,
    pub position: BlockPos,
    pub sequence: i32,
}

#[derive(Event)]
pub struct BlockFinishBreak {
    #[event(target)]
    pub by: EntityId,
    pub position: BlockPos,
    pub sequence: i32,
}

#[derive(Event, Debug)]
pub struct UpdateBlock {
    pub position: BlockPos,
    pub id: BlockState,
    pub sequence: i32,
}

#[derive(Event)]
pub struct ChatMessage {
    #[event(target)]
    pub target: EntityId,
    pub message: Text,
}

#[derive(Event)]
pub struct DisguisePlayer {
    #[event(target)]
    pub target: EntityId,
    pub mob: EntityKind,
}

#[derive(Component, Deref, DerefMut, Default)]
pub struct Scratches {
    inner: RayonLocal<RefCell<Scratch>>,
}

/// This often only displays the effect. For instance, for speed it does not give the actual speed effect.
#[derive(Event, Copy, Clone)]
pub struct DisplayPotionEffect {
    #[event(target)]
    pub target: EntityId,
    pub effect: StatusEffect,
    pub amplifier: u8,
    pub duration: i32,

    // todo: make this a bitfield
    ///  whether or not this is an effect provided by a beacon and therefore should be less intrusive on the screen.
    /// Optional, and defaults to false.
    pub ambient: bool,
    pub show_particles: bool,
    pub show_icon: bool,
}

#[derive(Event, Copy, Clone)]
pub struct SpeedEffect {
    #[event(target)]
    target: EntityId,
    level: u8,
}

impl SpeedEffect {
    #[must_use]
    pub const fn new(target: EntityId, level: u8) -> Self {
        Self { target, level }
    }

    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }
}

// todo: why need two life times?
#[derive(Event)]
pub struct Gametick<'a> {
    pub bump: &'a Bump,
    pub player_bounding_boxes: bvh_region::Bvh<LookupData, &'a Bump>,
    pub entity_bounding_boxes: bvh_region::Bvh<LookupData, &'a Bump>,
}

impl<'a> Gametick<'a> {
    pub fn new(bump: &'a Bump) -> Self {
        Self {
            bump,
            player_bounding_boxes: bvh_region::Bvh::null_in(bump),
            entity_bounding_boxes: bvh_region::Bvh::null_in(bump),
        }
    }

    #[must_use]
    pub const fn allocator(&self) -> &'a Bump {
        self.bump
    }
}

/// An event that is sent when it is time to send packets to clients.
#[derive(Event)]
pub struct Egress<'a> {
    pub server: &'a mut Server,
}

#[derive(Event)]
pub struct SetPlayerSkin {
    #[event(target)]
    pub target: EntityId,
    pub skin: PlayerSkin,
}

#[derive(Event)]
pub struct PointCompass {
    #[event(target)]
    pub target: EntityId,
    pub point_to: BlockPos,
}

#[derive(Event)]
pub struct SetEquipment<'a> {
    #[event(target)]
    pub target: EntityId,
    pub equipment: Cow<'a, [EquipmentEntry]>,
}

impl<'a> SetEquipment<'a> {
    pub fn new(target: EntityId, equipment: impl Into<Cow<'a, [EquipmentEntry]>>) -> Self {
        let equipment = equipment.into();
        Self { target, equipment }
    }
}
