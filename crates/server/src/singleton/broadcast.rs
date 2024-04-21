//! Defines a singleton that is used to broadcast packets to all players.

// https://stackoverflow.com/a/61681112/4889030
// https://matklad.github.io/2020/10/03/fast-thread-locals-in-rust.html

use uuid::Uuid;
use valence_protocol::math::Vec2;

/// A definition of whether a packet is always required or can be dropped.
///
/// This is useful when a player has a limited amount of bandwidth and we want to prioritize
/// sending packets to the player.
#[derive(Copy, Clone)]
pub enum PacketNecessity {
    /// The packet is always required and cannot be dropped. An example would be an entity spawn packet.
    Required,

    /// The packet is optional and can be dropped. An example would be a player position packet, entity movement packet, etc.
    Droppable {
        /// The location to prioritize the packet at. If this is an entity movement packet, this is the location of the entity.
        /// This will mean
        /// that the packet is more likely to be sent to players near to this location if their bandwidth is limited.
        #[expect(
            dead_code,
            reason = "this is not used, but we plan to use it in the future"
        )]
        prioritize_location: Vec2,
    },
}

/// Metadata for determining how to send a packet.
#[derive(Copy, Clone)]
#[expect(
    dead_code,
    reason = "this is not used, but we plan to use it in the future"
)]
pub struct PacketMetadata {
    /// Determines whether the packet is required or optional.
    pub necessity: PacketNecessity,
    /// The player to exclude from the packet.
    /// For instance, if a player is broadcasting their own position,
    /// they should not be included in the broadcast of that packet.
    ///
    /// todo: implement `exclude_player` and use a more efficient option (perhaps a global packet bitmask)
    pub exclude_player: Option<Uuid>,
}

impl PacketMetadata {
    /// The server can drop the packet (with no prioritization of location).
    #[expect(
        dead_code,
        reason = "this is not used, but we plan to use it in the future"
    )]
    pub const DROPPABLE: Self = Self {
        necessity: PacketNecessity::Droppable {
            prioritize_location: Vec2::new(0.0, 0.0),
        },
        exclude_player: None,
    };
    /// The packet is required.
    #[expect(
        dead_code,
        reason = "this is not used, but we plan to use it in the future"
    )]
    pub const REQUIRED: Self = Self {
        necessity: PacketNecessity::Required,
        exclude_player: None,
    };
}
