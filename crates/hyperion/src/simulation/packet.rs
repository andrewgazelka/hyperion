use std::collections::HashMap;

use anyhow::Result;
use rustc_hash::FxBuildHasher;
use valence_protocol::{Decode, Packet, packets::play::ChatMessageC2s};

type TempAny<'a> = Box<dyn std::any::Any + Send + Sync + 'a>;

// We'll store the deserialization function separately from handlers
type DeserializerFn = Box<dyn for<'a> Fn(&'a [u8]) -> Result<TempAny<'a>> + 'static>;

type PacketHandler = Box<dyn Fn(&(dyn std::any::Any + Send + Sync)) -> Result<()>>;

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
    pub fn register_packet<P>(&mut self)
    where
        P: Packet + Send + Sync + for<'a> Decode<'a>,
    {
        let deserializer: DeserializerFn = Box::new(
            |mut bytes: &[u8]| -> Result<Box<dyn std::any::Any + Send + Sync>> {
                let bytes = &mut bytes;
                let packet = P::decode(bytes)?;
                Ok(Box::new(packet))
            },
        );

        self.deserializers.insert(P::ID, deserializer);
        // Initialize the handlers vector if it doesn't exist
        self.handlers.entry(P::ID).or_insert_with(Vec::new);
    }

    // Add a handler for a packet type
    pub fn add_handler<P>(
        &mut self,
        handler: impl for<'a> Fn(&'a P) -> Result<()> + Send + Sync + 'static,
    ) where
        P: Packet + Send + Sync + for<'a> Decode<'a>,
    {
        // Ensure the packet type is registered
        if !self.deserializers.contains_key(&P::ID) {
            self.register_packet::<P>();
        }

        // Wrap the typed handler to work with Any
        let boxed_handler: PacketHandler = Box::new(move |any_packet| {
            let packet = any_packet
                .downcast_ref::<P>()
                .ok_or_else(|| anyhow::anyhow!("Invalid packet type"))?;
            handler(packet)
        });

        // Add the handler to the vector
        self.handlers.entry(P::ID).or_default().push(boxed_handler);
    }

    // Process a packet, calling all registered handlers
    pub fn process_packet<'a>(&self, id: i32, bytes: &'a [u8]) -> Result<()> {
        // Get the deserializer
        let deserializer = self
            .deserializers
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("No deserializer registered for packet ID: {}", id))?;

        // Deserialize the packet once
        let packet = deserializer(bytes)?;

        // Get all handlers for this packet type
        // let handlers = self
        //     .handlers
        //     .get(&id)
        //     .ok_or_else(|| anyhow::anyhow!("No handlers registered for packet ID: {}", id))?;
        //
        // // Call all handlers with the deserialized packet
        // for handler in handlers {
        //     handler(&*packet)?;
        // }

        Ok(())
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
