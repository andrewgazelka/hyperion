#![feature(allocator_api)]
#![feature(let_chains)]
#![feature(coroutines)]
#![feature(stmt_expr_attributes)]
#![feature(coroutine_trait)]
#![feature(iter_from_coroutine)]

use std::net::ToSocketAddrs;

use flecs_ecs::prelude::*;
use hyperion::{simulation::Player, Hyperion};
use module::block::BlockModule;

mod component;
mod module;

use module::{
    attack::AttackModule, command::CommandModule, level::LevelModule,
    regeneration::RegenerationModule,
};

use crate::module::stats::StatsModule;

#[derive(Component)]
pub struct InfectionModule;

impl Module for InfectionModule {
    fn module(world: &World) {
        world.component::<component::team::Team>();

        world
            .component::<Player>()
            .add_trait::<(flecs::With, component::team::Team)>();

        world.import::<CommandModule>();
        world.import::<StatsModule>();
        world.import::<BlockModule>();
        world.import::<AttackModule>();
        world.import::<LevelModule>();
        world.import::<RegenerationModule>();
    }
}

pub fn init_game(address: impl ToSocketAddrs + Send + Sync + 'static) -> anyhow::Result<()> {
    Hyperion::init_with(address, |world| {
        world.import::<InfectionModule>();
    })?;

    Ok(())
}
