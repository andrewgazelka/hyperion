use std::{cell::RefCell, rc::Rc};

use flecs_ecs::{
    core::{flecs, SystemAPI, World},
    macros::Component,
    prelude::Module,
};
use hyperion::{
    simulation::{Position, Uuid},
    valence_protocol::math::Vec3,
};
use rustc_hash::FxHashMap;

#[derive(Component)]
pub struct SpawnModule;

const MIN_RADIUS: i32 = 0;
const MAX_RADIUS: i32 = 400;
// const SPAWN_Y: f32 = -21.0;
const SPAWN_Y: f32 = 100.0;

fn random_position() -> Vec3 {
    let r = fastrand::i32(MIN_RADIUS..=MAX_RADIUS) as f32;
    let theta = fastrand::f32() * 2.0 * std::f32::consts::PI;

    let x = r * theta.cos();
    let z = r * theta.sin();

    Vec3::new(x, SPAWN_Y, z)
}

impl Module for SpawnModule {
    fn module(world: &World) {
        let positions = Rc::new(RefCell::new(FxHashMap::default()));

        world.observer::<flecs::OnSet, &Uuid>().each_entity({
            let positions = Rc::clone(&positions);
            move |entity, uuid| {
                let mut positions = positions.borrow_mut();
                let position = *positions.entry(uuid.0).or_insert_with(random_position);

                entity.set(Position::from(position));

                println!("got uuid: {uuid:?}");
            }
        });

        world
            .observer::<flecs::OnRemove, (&Uuid, &Position)>()
            .each(move |(uuid, position)| {
                let mut positions = positions.borrow_mut();
                positions.insert(uuid.0, **position);
            });
    }
}
