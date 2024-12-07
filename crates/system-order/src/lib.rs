use std::collections::{BTreeSet, HashMap};

use derive_more::Constructor;
use flecs_ecs::{
    core::{
        flecs, flecs::DependsOn, Builder, Entity, EntityView, EntityViewGet, IdOperations,
        QueryAPI, QueryBuilderImpl,
    },
    macros::Component,
    prelude::{Module, World},
};

/// sort by depth and then by id
#[derive(PartialOrd, Ord, PartialEq, Eq, Debug)]
struct OrderKey {
    depth: usize,
    id: Entity,
}

#[derive(Default)]
struct DepthCalculator {
    depths: HashMap<Entity, usize, rustc_hash::FxBuildHasher>,
}

impl DepthCalculator {
    fn calculate_depth(&mut self, view: EntityView<'_>) -> usize {
        if let Some(depth) = self.depths.get(&view.id()) {
            return *depth;
        }

        // todo: add stackoverflow check
        let mut entity_depth = 0;

        view.each_target::<DependsOn>(|depends_on| {
            let tentative_depth = self.calculate_depth(depends_on) + 1;
            entity_depth = entity_depth.max(tentative_depth);
        });

        self.depths.insert(view.id(), entity_depth);

        entity_depth
    }
}

#[derive(
    Component,
    Constructor,
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord
)]
#[must_use]
#[meta]
pub struct SystemOrder {
    value: u16,
}

impl SystemOrder {
    #[must_use]
    pub const fn value(&self) -> u16 {
        self.value
    }

    pub fn of(entity: EntityView<'_>) -> Self {
        entity.get::<&Self>(|order| *order)
    }
}

fn calculate(world: &World) {
    let mut depth_calculator = DepthCalculator::default();

    let mut map = BTreeSet::new();

    // get all depths for systems
    world
        .query::<()>()
        .with::<flecs::system::System>()
        .build()
        .each_entity(|entity, ()| {
            let depth = depth_calculator.calculate_depth(entity);

            map.insert(OrderKey {
                depth,
                id: entity.id(),
            });
        });

    let mut observer_ids = Vec::new();

    // handle all observers as having order u16::MAX (so they are executed last)
    world
        .query::<()>()
        .with::<flecs::Observer>()
        .build()
        .each_entity(|entity, ()| {
            observer_ids.push(entity.id());
        });

    for id in observer_ids {
        let entity = id.entity_view(world);
        entity.set(SystemOrder::new(u16::MAX));
    }

    for (idx, value) in map.into_iter().enumerate() {
        let idx = u16::try_from(idx).expect("number of systems exceeds u16 (65536)");

        let entity = value.id.entity_view(world);

        entity.set(SystemOrder::new(idx));
    }
}

#[derive(Component)]
pub struct SystemOrderModule;

impl Module for SystemOrderModule {
    fn module(world: &World) {
        world.component::<SystemOrder>().meta();

        calculate(world);
    }
}
