use derive_more::{Deref, DerefMut};
use evenio::component::Component;
use valence_protocol::PacketDecoder;

#[derive(Component, Deref, DerefMut, Default)]
pub struct PlayerPacketBuffer {
    decoder: PacketDecoder,
}
