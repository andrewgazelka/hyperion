use std::{collections::HashMap, ptr::NonNull};

use anyhow::Result;
use rustc_hash::FxBuildHasher;
use valence_protocol::{Decode, Packet, packets::play::ChatMessageC2s};

// We'll store the deserialization function separately from handlers
type DeserializerFn = Box<dyn Fn(&HandlerRegistry, &[u8]) -> Result<()>>;

type PacketHandler = Box<dyn Fn(NonNull<u8>) -> Result<()>>;

pub struct HandlerRegistry {
    // Store deserializer and multiple handlers separately
    deserializers: HashMap<i32, DeserializerFn, FxBuildHasher>,
    handlers: HashMap<i32, Vec<PacketHandler>, FxBuildHasher>,
}

impl HandlerRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            deserializers: HashMap::default(),
            handlers: HashMap::default(),
        }
    }

    // Register a packet type's deserializer
    pub fn register_packet<'p, P>(&mut self)
    where
        P: Packet + Send + Sync + Decode<'p>,
    {
        let deserializer: DeserializerFn =
            Box::new(|registry: &Self, bytes: &[u8]| -> Result<()> {
                // transmute to bypass lifetime issue with Decode
                // packet is dropped at end of scope and references in handlers are locked to the scope of the handler
                let mut bytes = unsafe { std::mem::transmute(bytes) };
                let mut packet = P::decode(&mut bytes)?;
                // packet is always non-null, swap to NonNull::from_mut after stabilization
                let ptr = unsafe { NonNull::new_unchecked(&mut packet).cast::<u8>() };

                // Get all handlers for this packet type
                let handlers = registry.handlers.get(&P::ID).ok_or_else(|| {
                    anyhow::anyhow!("No handlers registered for packet ID: {}", P::ID)
                })?;

                // Call all handlers with the deserialized packet
                for handler in handlers {
                    handler(ptr)?;
                }

                Ok(())
            });

        self.deserializers.insert(P::ID, deserializer);
        // Initialize the handlers vector if it doesn't exist
        self.handlers.entry(P::ID).or_insert_with(Vec::new);
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

        deserializer(self, bytes)
    }
}

fn main() -> Result<()> {
    let mut registry = HandlerRegistry::new();

    // Register multiple handlers for ChatPacket
    registry.add_handler::<ChatMessageC2s<'_>>(|packet| {
        println!("Handler 1: {:?}", packet);
        Ok(())
    });

    Ok(())
}
