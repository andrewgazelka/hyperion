#![feature(allocator_api)]
#![feature(let_chains)]
#![feature(coroutines)]
#![feature(stmt_expr_attributes)]
#![feature(coroutine_trait)]
#![feature(iter_from_coroutine)]

use std::net::ToSocketAddrs;

use flecs_ecs::prelude::*;
use hyperion::{storage::GlobalEventHandlers, Hyperion};

mod animation;
mod command;
mod component;
mod handler;

pub use animation::AnimationModule;
use command::CommandModule;

#[derive(Component)]
pub struct InfectionModule;

impl Module for InfectionModule {
    fn module(world: &World) {
        world.component::<component::team::Team>();

        world.import::<CommandModule>();
        // world.import::<AnimationModule>();
    }
}

pub fn init_game(address: impl ToSocketAddrs + Send + Sync + 'static) -> anyhow::Result<()> {
    Hyperion::init_with(address, |world| {
        world.get::<&mut GlobalEventHandlers>(|handlers| {
            handlers.join_server.register(handler::add_player_to_team);
        });

        world.import::<InfectionModule>();
    })?;

    Ok(())
}
