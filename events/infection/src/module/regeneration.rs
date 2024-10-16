use flecs_ecs::{
    core::{flecs, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    net::Compose,
    simulation::{Health, Player},
};

#[derive(Component)]
pub struct RegenerationModule;

#[derive(Component, Default, Copy, Clone, Debug)]
pub struct LastDamaged {
    pub tick: i64,
}

impl Module for RegenerationModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        world
            .component::<Player>()
            .add_trait::<(flecs::With, LastDamaged)>(); // todo: how does this even call Default? (IndraDb)

        system!("regenerate", world, &mut LastDamaged, &mut Health, &Compose($))
            .multi_threaded()
            .each(|(last_damaged, health, compose)| {
                let current_tick = compose.global().tick;

                if health.just_damaged() {
                    last_damaged.tick = current_tick;
                }

                let ticks_since_damage = current_tick - last_damaged.tick;

                // Calculate regeneration rate based on time since last damage
                let base_regen = 0.01; // Base regeneration per tick
                let ramp_factor = 0.0001_f32; // Increase in regeneration per tick
                let max_regen = 0.1; // Maximum regeneration per tick

                let regen_rate = ramp_factor
                    .mul_add(ticks_since_damage as f32, base_regen)
                    .min(max_regen);

                // Apply regeneration, capped at max health
                health.heal(regen_rate);
            });
    }
}
