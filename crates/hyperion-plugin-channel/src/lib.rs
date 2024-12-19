use std::collections::HashMap;

use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TermBuilderImpl, World},
    macros::{Component, system},
    prelude::Module,
};
use hyperion::{
    HyperionCore,
    simulation::event::PluginMessage,
    storage::{BoxedEventFn, EventFn, EventQueue},
    valence_ident::Ident,
};

#[derive(Component)]
struct PluginChannelModule;

#[derive(Component, Default)]
struct PluginChannelRegistry {
    registry: HashMap<Ident<String>, BoxedEventFn<[u8]>>,
}

impl PluginChannelRegistry {
    pub fn register(&mut self, name: impl Into<Ident<String>>, handler: impl EventFn<[u8]>) {
        let name = name.into();
        self.registry.insert(name, EventFn::boxed(handler));
    }
}

impl Module for PluginChannelModule {
    fn module(world: &World) {
        world.import::<HyperionCore>();

        world.component::<PluginChannelRegistry>();
        world.add::<PluginChannelRegistry>();

        system!(
            "process-plugin-messages",
            world,
            &mut EventQueue<PluginMessage<'static>>($),
            &PluginChannelRegistry($)
        )
        .each_iter(|it, _, (queue, registry)| {
            for PluginMessage { channel, data } in queue.drain() {
                let Some(handler) = registry.registry.get(channel) else {
                    continue;
                };

                handler(todo!(), data);
            }
        });
        // world.get::<&mut EventQueue<PluginMessage<'static>>>(move |queue|
        //     for PluginMessage { channel, data } in queue.drain() {

        //     }
        // });
    }
}

#[cfg(test)]
mod tests {
    use flecs_ecs::core::WorldGet;
    use hyperion::{
        valence_ident::ident,
        valence_protocol::{RawBytes, packets::play},
    };

    use super::*;

    #[test]
    fn test_echo() {
        let world = World::new();
        world.import::<PluginChannelModule>();

        world.get::<&mut PluginChannelRegistry>(|registry| {
            registry.register(ident!("hyperion:echo"), |query, data| {
                let data = RawBytes::from(data);
                let response_packet = play::CustomPayloadS2c {
                    channel: ident!("hyperion:echo").into(),
                    data: data.into(),
                };

                query
                    .compose
                    .unicast(&response_packet, query.io_ref, query.system)
                    .unwrap();
            });
        });
    }
}
