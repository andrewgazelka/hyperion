#![feature(allocator_api)]
#![feature(let_chains)]
#![feature(coroutines)]
#![feature(stmt_expr_attributes)]
#![feature(coroutine_trait)]
#![feature(iter_from_coroutine)]
#![feature(exact_size_is_empty)]

use std::net::ToSocketAddrs;

use clap::Parser;
use flecs_ecs::prelude::*;
use hyperion::{
    Hyperion,
    net::{Compose, DataBundle, NetworkStreamRef, agnostic},
    runtime::AsyncRuntime,
    simulation::{Player, blocks::Blocks},
    system_registry::SystemId,
    valence_protocol::packets::play::{
        PlayerAbilitiesS2c, player_abilities_s2c::PlayerAbilitiesFlags,
    },
};
use hyperion_clap::{MinecraftCommand, hyperion_command::CommandRegistry};
use module::block::BlockModule;

mod component;
mod module;

use module::{attack::AttackModule, level::LevelModule, regeneration::RegenerationModule};

use crate::module::{chat::ChatModule, spawn::SpawnModule, stats::StatsModule};

#[derive(Component)]
pub struct ProofOfConceptModule;

#[derive(Parser, Debug)]
#[command(name = "speed")]
struct SpeedCommand {
    amount: f32,
}

impl MinecraftCommand for SpeedCommand {
    fn execute(self, world: &World, caller: Entity) {
        let msg = format!("Setting speed to {}", self.amount);
        let chat = agnostic::chat(msg);

        world.get::<&Compose>(|compose| {
            caller
                .entity_view(world)
                .get::<&NetworkStreamRef>(|stream| {
                    let packet = fly_speed_packet(self.amount);

                    let mut bundle = DataBundle::new(compose);
                    bundle.add_packet(&packet, world).unwrap();
                    bundle.add_packet(&chat, world).unwrap();

                    bundle.send(world, *stream, SystemId(8)).unwrap();
                });
        });
    }
}

fn fly_speed_packet(amount: f32) -> PlayerAbilitiesS2c {
    PlayerAbilitiesS2c {
        flags: PlayerAbilitiesFlags::default()
            .with_allow_flying(true)
            .with_flying(true),
        flying_speed: amount,
        fov_modifier: 0.0,
    }
}

impl Module for ProofOfConceptModule {
    fn module(world: &World) {
        world.component::<component::team::Team>();

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

        world.get::<&mut CommandRegistry>(|registry| {
            SpeedCommand::register(registry, world);
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
