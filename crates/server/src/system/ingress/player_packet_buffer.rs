use derive_more::{Deref, DerefMut};
use evenio::component::Component;

use crate::net::PacketDecoder;

#[derive(Component, Deref, DerefMut, Default)]
pub struct DecodeBuffer {
    decoder: PacketDecoder,
}
