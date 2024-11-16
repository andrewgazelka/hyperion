use clap::ValueEnum;
use flecs_ecs::{
    core::{Entity, IdOperations, World, WorldGet},
    macros::Component,
    prelude::Module,
};
use hyperion::{net::Compose, storage::EventHandler};
use valence_protocol::Hand;

pub mod inventory;
pub mod skin;

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq, Component)]
#[repr(C)]
pub enum Rank {
    /// ![Widget Example](https://i.imgur.com/pW7v0Xn.png)
    ///
    /// The stick is the starting rank.
    Stick, // -> [Pickaxe | Sword | Bow ]

    Archer,
    Sword,
    Miner,

    Excavator,

    Mage,
    Knight,
    Builder,
}

pub enum Team {
    Red,
    White,
    Blue,
}

#[derive(Component)]
pub struct RankTree;

#[derive(Component)]
pub struct Handles {
    pub speed: Entity,
}

impl Module for RankTree {
    fn module(world: &World) {
        world.import::<hyperion_item::ItemModule>();
        world.component::<Rank>();
        world.component::<Handles>();

        let handler = EventHandler::new(|query, hand| {
            let cursor = query.inventory.get_cursor();
            println!("clicked {cursor:?}");
        });

        let speed = world.entity().set(hyperion_item::Handler::new(handler));

        world.set(Handles { speed: speed.id() });
    }
}
