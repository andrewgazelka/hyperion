//! Agnostic networking primitives. Translates to correct protocol version.

mod chat;
pub use chat::{chat, Chat};

mod sound;
pub use sound::{sound, Sound, SoundBuilder};
