use std::{
    alloc::{Layout, dealloc},
    collections::HashMap,
    ptr::NonNull,
};

use anyhow::Result;
use rustc_hash::FxBuildHasher;
use valence_protocol::{Decode, Packet, packets::play::ChatMessageC2s};

// We'll store the deserialization function separately from handlers
type DeserializerFn = Box<dyn Fn(&[u8]) -> Result<NonNull<u8>>>;

type PacketHandler = Box<dyn Fn(NonNull<u8>) -> Result<()>>;

pub struct HandlerRegistry {
    // Store deserializer and multiple handlers separately
    deserializers: HashMap<i32, DeserializerFn, FxBuildHasher>,
    handlers: HashMap<i32, Vec<PacketHandler>, FxBuildHasher>,
    droppers: HashMap<i32, Option<unsafe fn(NonNull<u8>)>, FxBuildHasher>,
}

impl HandlerRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            deserializers: HashMap::default(),
            handlers: HashMap::default(),
            droppers: HashMap::default(),
        }
    }

    // Register a packet type's deserializer
    pub fn register_packet<'p, P>(&mut self)
    where
        P: Packet + Send + Sync + Decode<'p>,
    {
        let deserializer: DeserializerFn = Box::new(|mut bytes: &[u8]| -> Result<NonNull<u8>> {
            let bytes = unsafe { std::mem::transmute(&mut bytes) };
            let packet = P::decode(bytes)?;
            let leaked = Box::leak(Box::new(packet));
            let ptr = unsafe { NonNull::new_unchecked(leaked).cast() };
            Ok(ptr)
        });

        self.deserializers.insert(P::ID, deserializer);
        // Initialize the handlers vector if it doesn't exist
        self.handlers.entry(P::ID).or_insert_with(Vec::new);

        self.droppers.insert(
            P::ID,
            std::mem::needs_drop::<P>().then_some(Self::dealloc_ptr::<P> as _),
        );
    }

    unsafe fn dealloc_ptr<T>(ptr: NonNull<u8>) {
        unsafe {
            ptr.cast::<T>().as_ptr().drop_in_place();
            dealloc(ptr.as_ptr(), Layout::new::<T>());
        }
    }

    // Add a handler for a packet type
    pub fn add_handler<'p, P>(
        &mut self,
        handler: impl for<'a> Fn(&'a P) -> Result<()> + Send + Sync + 'static,
    ) where
        P: Packet + Send + Sync + Decode<'p>,
    {
        // Ensure the packet type is registered
        if !self.deserializers.contains_key(&P::ID) {
            self.register_packet::<P>();
        }

        // Wrap the typed handler to work with Any
        let boxed_handler: PacketHandler = Box::new(move |any_packet| {
            let packet = unsafe { any_packet.cast::<P>().as_ref() };
            handler(packet)
        });

        // Add the handler to the vector
        self.handlers.entry(P::ID).or_default().push(boxed_handler);
    }

    // Process a packet, calling all registered handlers
    pub fn process_packet(&self, id: i32, bytes: &[u8]) -> Result<()> {
        // Get the deserializer
        let deserializer = self
            .deserializers
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("No deserializer registered for packet ID: {}", id))?;

        let dropper = self
            .droppers
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("No deserializer registered for packet ID: {}", id))?;

        // Deserialize the packet once
        let packet = deserializer(bytes)?;

        // Get all handlers for this packet type
        let handlers = self
            .handlers
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("No handlers registered for packet ID: {}", id))?;

        // Call all handlers with the deserialized packet
        for handler in handlers {
            handler(packet)?;
        }

        if let Some(drop) = dropper {
            unsafe { drop(packet) };
        }

        unsafe { drop(Box::from_raw(packet.as_ptr())) };

        Ok(())
    }
}

#[test]
fn main() -> Result<()> {
    let mut registry = HandlerRegistry::new();

    // Register multiple handlers for ChatPacket
    registry.add_handler::<ChatMessageC2s<'_>>(|packet| {
        println!("Handler 1: {:?}", packet);
        Ok(())
    });

    Ok(())
}
