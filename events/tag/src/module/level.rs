use flecs_ecs::{
    core::{SystemAPI, World, WorldProvider, flecs},
    macros::Component,
    prelude::Module,
};
use hyperion::{
    Prev,
    simulation::{Player, Xp},
};
use hyperion_inventory::PlayerInventory;
use hyperion_rank_tree::{Rank, Team};

use crate::MainBlockCount;

#[derive(Component)]
pub struct LevelModule;

#[derive(Component, Default, Copy, Clone, Debug)]
#[meta]
pub struct UpgradedTo {
    pub value: u8,
}

impl Module for LevelModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        world.component::<UpgradedTo>().meta();
        world
            .component::<Player>()
            .add_trait::<(flecs::With, UpgradedTo)>(); // todo: how does this even call Default? (IndraDb)

        // on Xp gain,
        world
            .system_named::<(
                &(Prev, Xp),
                &Xp,
                &UpgradedTo,
                &Rank,
                &Team,
                &MainBlockCount,
                &mut PlayerInventory,
            )>("level_up")
            .multi_threaded()
            .kind::<flecs::pipeline::PreStore>()
            .each_entity(
                |entity, (prev, xp, upgraded_to, rank, team, main_block_count, inventory)| {
                    if *xp <= *prev {
                        // we are only considering gains
                        return;
                    }

                    let prev_level = prev.get_visual().level;
                    let new_level = xp.get_visual().level;

                    if new_level <= prev_level {
                        // only considering our level increasing
                        return;
                    }

                    let world = entity.world();

                    let level_diff = new_level - upgraded_to.value;

                    rank.apply_inventory(*team, inventory, &world, **main_block_count, level_diff);
                },
            );
    }
}
