use std::{alloc::Allocator, fmt::Debug};

use bumpalo::Bump;
use evenio::{entity::EntityId, event::Event};
use glam::Vec3;
use rayon_local::RayonLocal;
use valence_generated::block::BlockState;
use valence_protocol::{BlockPos, Hand};
use valence_server::entity::EntityKind;
use valence_text::Text;

use crate::{
    components::FullEntityPose,
    net::{Server, MAX_PACKET_SIZE},
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
pub struct HurtEntity {
    #[event(target)]
    pub target: EntityId,
    pub damage: f32,
}

pub enum AttackType {
    Shove, Melee
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
#[derive(Event)]
pub struct Shoved {
    #[event(target)]
    pub target: EntityId,
    pub from: EntityId,
    pub from_location: Vec3,
}

/// An event when server stats are updated.
#[derive(Event)]
pub struct Stats<'a, 'b> {
    /// The number of milliseconds per tick in the last second.
    pub ms_per_tick_mean_1s: f64,
    /// The number of milliseconds per tick in the last 5 seconds.
    pub ms_per_tick_mean_5s: f64,

    pub scratch: &'b mut BumpScratch<'a>,
}

// todo: REMOVE
#[expect(
    clippy::non_send_fields_in_send_ty,
    reason = "this will be removed in the future"
)]
unsafe impl<'a, 'b> Send for Stats<'a, 'b> {}
unsafe impl<'a, 'b> Sync for Stats<'a, 'b> {}

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

// todo: why need two life times?
#[derive(Event)]
pub struct Gametick<'a, 'b> {
    pub bump: &'a RayonLocal<Bump>,
    pub scratch: &'b mut RayonLocal<BumpScratch<'a>>,
}

unsafe impl<'a, 'b> Send for Gametick<'a, 'b> {}
unsafe impl<'a, 'b> Sync for Gametick<'a, 'b> {}

/// An event that is sent when it is time to send packets to clients.
#[derive(Event)]
pub struct Egress<'a> {
    pub server: &'a mut Server,
}

// todo: remove
#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "this will be removed in the future"
)]
unsafe impl<'a> Send for Egress<'a> {}
unsafe impl<'a> Sync for Egress<'a> {}
