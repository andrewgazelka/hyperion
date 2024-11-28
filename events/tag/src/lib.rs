#![feature(allocator_api)]
#![feature(let_chains)]
#![feature(coroutines)]
#![feature(stmt_expr_attributes)]
#![feature(coroutine_trait)]
#![feature(iter_from_coroutine)]
#![feature(exact_size_is_empty)]

use std::net::ToSocketAddrs;

use flecs_ecs::prelude::*;
use hyperion::{
    Hyperion,
    runtime::AsyncRuntime,
    simulation::{Player, blocks::Blocks},
};
use hyperion_clap::hyperion_command::CommandRegistry;
use module::block::BlockModule;

mod component;
mod module;

use derive_more::{Deref, DerefMut};
use hyperion::glam::IVec3;
use module::{attack::AttackModule, level::LevelModule, regeneration::RegenerationModule};

use crate::{
    module::{chat::ChatModule, spawn::SpawnModule, stats::StatsModule},
    skin::SkinModule,
};

#[derive(Component)]
pub struct ProofOfConceptModule;

mod command;
mod skin;

#[derive(Component, Default, Deref, DerefMut)]
struct OreVeins {
    ores: gxhash::HashSet<IVec3>,
}

#[derive(Component, Deref, DerefMut)]
struct MainBlockCount(i8);

impl Default for MainBlockCount {
    fn default() -> Self {
        Self(16)
    }
}

impl Module for ProofOfConceptModule {
    fn module(world: &World) {
        world.component::<MainBlockCount>();

        world
            .component::<Player>()
            .add_trait::<(flecs::With, MainBlockCount)>();

        world.component::<component::team::Team>();
        world.import::<hyperion_rank_tree::RankTree>();

        world.component::<OreVeins>();
        world.set(OreVeins::default());

        world
            .component::<Player>()
            .add_trait::<(flecs::With, component::team::Team)>();

        world.import::<SpawnModule>();
        world.import::<ChatModule>();
        world.import::<StatsModule>();
        world.import::<BlockModule>();
        world.import::<AttackModule>();
        world.import::<LevelModule>();
        world.import::<RegenerationModule>();
        world.import::<hyperion_permission::PermissionModule>();
        world.import::<hyperion_utils::HyperionUtilsModule>();
        world.import::<hyperion_clap::ClapCommandModule>();
        world.import::<SkinModule>();

        world.get::<&mut CommandRegistry>(|registry| {
            command::register(registry, world);
        });

        world.set(hyperion_utils::AppId {
            qualifier: "com".to_string(),
            organization: "andrewgazelka".to_string(),
            application: "hyperion-poc".to_string(),
        });

        world.get::<&AsyncRuntime>(|runtime| {
            let f = hyperion_utils::cached_save(
                world,
                "https://github.com/andrewgazelka/maps/raw/main/GenMap.tar.gz",
            );

            runtime.schedule(f, |result, world| {
                let save = result.unwrap();
                world.set(Blocks::new(world, &save).unwrap());
            });
        });
    }
}

pub fn init_game(address: impl ToSocketAddrs + Send + Sync + 'static) -> anyhow::Result<()> {
    Hyperion::init_with(address, |world| {
        world.import::<ProofOfConceptModule>();
    })?;

    Ok(())
}
