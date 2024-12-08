use bvh_region::TrivialHeuristic;
use flecs_ecs::{
    core::{
        flecs, Builder, Entity, EntityView, EntityViewGet, IdOperations, QueryAPI,
        QueryBuilderImpl, SystemAPI, TermBuilderImpl, World, WorldGet,
    },
    macros::{system, Component},
    prelude::Module,
};
use geometry::aabb::Aabb;
use hyperion::{
    egress::player_join::RayonWorldStages,
    glam::Vec3,
    simulation::{aabb, EntitySize, Position},
};

#[derive(Component)]
pub struct SpatialModule;

#[derive(Component, Debug, Default)]
pub struct SpatialIndex {
    /// The bounding boxes of all players
    query: bvh_region::Bvh<Entity>,
}

fn get_aabb_func<'a>(world: &'a World) -> impl Fn(&Entity) -> Aabb + Send + Sync + use<'a> {
    let stages: &'a RayonWorldStages = world.get::<&RayonWorldStages>(|stages| {
        // we can properly extend lifetimes here
        unsafe { core::mem::transmute(stages) }
    });

    |entity: &Entity| {
        let rayon_thread = rayon::current_thread_index().unwrap_or_default();

        stages[rayon_thread]
            .entity_from_id(*entity)
            .get::<(&Position, &EntitySize)>(|(position, size)| aabb(**position, *size))
    }
}

impl SpatialIndex {
    fn recalculate(&mut self, world: &World) {
        let all_entities = all_indexed_entities(world);
        let get_aabb = get_aabb_func(world);

        self.query = bvh_region::Bvh::build::<TrivialHeuristic>(all_entities, &get_aabb);
    }

    // pub fn get_collisions<'a>(
    //     &'a self,
    //     target: Aabb,
    //     world: &'a World,
    // ) -> impl Iterator<Item = Entity> + 'a {
    //     let get_aabb = get_aabb_func(world);
    //     self.query.get_collisions(target, get_aabb).copied()
    // }

    /// Get the closest player to the given position.
    #[must_use]
    pub fn closest_to<'a>(&self, point: Vec3, world: &'a World) -> Option<EntityView<'a>> {
        let get_aabb = get_aabb_func(world);
        let (target, _) = self.query.get_closest(point, &get_aabb)?;
        Some(world.entity_from_id(*target))
    }
}
//
/// If we want the entity to be spatially indexed, we need to add this component.
#[derive(Component)]
pub struct Spatial;
// todo(perf): re-use allocations?
fn all_indexed_entities(world: &World) -> Vec<Entity> {
    // todo(perf): can we cache this?
    let query = world
        .query::<()>()
        .with::<Position>()
        .with::<EntitySize>()
        .with::<Spatial>()
        .build();

    let count = query.count();
    let count = usize::try_from(count).unwrap();
    let mut entities = Vec::with_capacity(count);

    query.each_entity(|entity, ()| {
        entities.push(entity.id());
    });

    entities
}
//
impl Module for SpatialModule {
    fn module(world: &World) {
        world.component::<Spatial>();
        world.component::<SpatialIndex>();
        world.add::<SpatialIndex>();

        system!(
            "recalculate_spatial_index",
            world,
            &mut SpatialIndex($),
        )
        .with::<flecs::pipeline::OnStore>()
        .each_iter(|it, _, index| {
            let world = it.world();
            index.recalculate(&world);
        });
    }
}
