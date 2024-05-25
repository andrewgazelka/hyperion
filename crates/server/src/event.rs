use std::{alloc::Allocator, cell::RefCell, fmt::Debug};

use bumpalo::Bump;
use derive_more::{Deref, DerefMut};
use evenio::{
    component::Component,
    entity::EntityId,
    event::{GlobalEvent, TargetedEvent},
};
use glam::Vec3;
use rayon_local::RayonLocal;
use valence_generated::{block::BlockState, status_effects::StatusEffect};
use valence_protocol::{packets::play::click_slot_c2s::SlotChange, BlockPos, Hand, ItemStack};
use valence_server::entity::EntityKind;
use valence_text::Text;

use crate::{
    components::FullEntityPose,
    net::{Server, MAX_PACKET_SIZE},
    util::player_skin::PlayerSkin,
};

#[derive(GlobalEvent, Debug)]
pub struct GenericBulkCollitionEvent {
    pub events: RayonLocal<Vec<Collision>>,
}

#[derive(Debug)]
/// Part of the [`GenericBulkCollitionEvent`] event.
/// This will be created by the generic collision system.
pub struct Collision {
    /// entit of the queried entity
    pub enitiy_id: EntityId,
    /// The colliding entity
    pub other_entity_id: EntityId,
}

#[derive(TargetedEvent, Debug)]
pub struct DropItem {
    pub drop_type: DropType,
}

#[derive(Debug)]
pub enum DropType {
    Single,
    All,
}

/// An event that is sent when a player clicks in the inventory.
#[derive(TargetedEvent, Debug)]
pub struct ClickEvent {
    pub click_type: ClickType,
    // maybe use smallvec to reduce heap allocations
    pub slot_changes: Vec<SlotChange>,
    pub carried_item: ItemStack,
}

/// The type of click that the player performed.
#[derive(Copy, Clone, Debug)]
pub enum ClickType {
    LeftClick { slot: i16 },
    RightClick { slot: i16 },
    LeftClickOutsideOfWindow,
    RightClickOutsideOfWindow,
    ShiftLeftClick { slot: i16 },
    ShiftRightClick { slot: i16 },
    HotbarKeyPress { button: i8, slot: i16 },
    OffHandSwap { slot: i16 },
    // todo: support for creative mode
    CreativeMiddleClick { slot: i16 },
    QDrop { slot: i16 },
    QControlDrop { slot: i16 },
    StartLeftMouseDrag,
    StartRightMouseDrag,
    StartMiddleMouseDrag,
    AddSlotLeftDrag { slot: i16 },
    AddSlotRightDrag { slot: i16 },
    AddSlotMiddleDrag { slot: i16 },
    EndLeftMouseDrag {},
    EndRightMouseDrag {},
    EndMiddleMouseDrag,
    DoubleClick { slot: i16 },
    DoubleClickReverseOrder { slot: i16 },
}

#[derive(TargetedEvent)]
/// An event that is sent when a player is changes his main hand
pub struct UpdateSelectedSlot {
    pub slot: u16,
}

/// This event is sent when the payer equipment gets sent to the client.
#[derive(TargetedEvent)]
pub struct UpdateEquipment;

/// Initialize a Minecraft entity (like a zombie) with a given pose.
#[derive(GlobalEvent)]
pub struct InitEntity {
    /// The pose of the entity.
    pub pose: FullEntityPose,
    pub display: EntityKind,
}

#[derive(TargetedEvent)]
pub struct Command {
    pub raw: String,
}

#[derive(TargetedEvent)]
pub struct PlayerInit {
    /// The name of the player i.e., `Emerald_Explorer`.
    pub username: Box<str>,
    pub pose: FullEntityPose,
}

/// Sent whenever a player joins the server.
#[derive(TargetedEvent)]
pub struct PlayerJoinWorld;

#[derive(TargetedEvent)]
pub struct PostPlayerJoinWorld;

/// An event that is sent whenever a player is kicked from the server.
#[derive(TargetedEvent)]
pub struct KickPlayer {
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

#[derive(TargetedEvent)]
#[event(immutable)]
pub struct PoseUpdate {
    pub state: Pose,
}

/// An event that is sent whenever a player swings an arm.
#[derive(TargetedEvent)]
pub struct SwingArm {
    /// The hand the player is swinging.
    pub hand: Hand,
}

#[derive(TargetedEvent)]
pub struct HurtEntity {
    pub damage: f32,
}

pub enum AttackType {
    Shove,
    Melee,
}

#[derive(TargetedEvent)]
pub struct AttackEntity {
    /// The location of the player that is hitting.
    pub from_pos: Vec3,
    pub from: EntityId,
    pub damage: f32,
    pub source: AttackType,
}

#[derive(TargetedEvent)]
#[event(immutable)]
pub struct Death;

/// An event to kill all minecraft entities (like zombies, skeletons, etc). This will be sent to the equivalent of
/// `/killall` in the game.
#[derive(GlobalEvent)]
pub struct KillAllEntities;

#[derive(TargetedEvent)]
pub struct Teleport {
    pub position: Vec3,
}

/// i.e., when zombies bump into another player
#[derive(Debug)]
pub struct Shoved {
    pub target: EntityId,
    pub from: EntityId,
    pub from_location: Vec3,
}

#[derive(GlobalEvent, Debug)]
pub struct BulkShoved(pub RayonLocal<Vec<Shoved>>);

/// An event when server stats are updated.
#[derive(GlobalEvent)]
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

pub type BumpScratch<'a> = Scratch<&'a Bump>;

impl<A: Allocator> From<A> for Scratch<A> {
    fn from(allocator: A) -> Self {
        Self {
            inner: Vec::with_capacity_in(MAX_PACKET_SIZE, allocator),
        }
    }
}

#[derive(TargetedEvent)]
pub struct BlockStartBreak {
    pub position: BlockPos,
    pub sequence: i32,
}

#[derive(TargetedEvent)]
pub struct BlockAbortBreak {
    pub position: BlockPos,
    pub sequence: i32,
}

#[derive(TargetedEvent)]
pub struct BlockFinishBreak {
    pub position: BlockPos,
    pub sequence: i32,
}

#[derive(GlobalEvent, Debug)]
pub struct UpdateBlock {
    pub position: BlockPos,
    pub id: BlockState,
    pub sequence: i32,
}

#[derive(TargetedEvent)]
pub struct ChatMessage {
    pub message: Text,
}

impl ChatMessage {
    pub fn new(message: impl Into<Text>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(TargetedEvent)]
pub struct DisguisePlayer {
    pub mob: EntityKind,
}

#[derive(Component, Deref, DerefMut, Default)]
pub struct Scratches {
    inner: RayonLocal<RefCell<Scratch>>,
}

/// This often only displays the effect. For instance, for speed it does not give the actual speed effect.
#[derive(TargetedEvent, Copy, Clone)]
pub struct DisplayPotionEffect {
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

#[derive(TargetedEvent, Copy, Clone)]
pub struct SpeedEffect {
    level: u8,
}

impl SpeedEffect {
    #[must_use]
    pub const fn new(level: u8) -> Self {
        Self { level }
    }

    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }
}

// todo: why need two life times?
#[derive(GlobalEvent)]
pub struct Gametick;

/// An event that is sent when it is time to send packets to clients.
#[derive(GlobalEvent)]
pub struct Egress<'a> {
    pub server: &'a mut Server,
}

#[derive(TargetedEvent)]
pub struct SetPlayerSkin {
    pub skin: PlayerSkin,
}

#[derive(TargetedEvent)]
pub struct PointCompass {
    pub point_to: BlockPos,
}
