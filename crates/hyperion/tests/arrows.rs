#![allow(
    clippy::print_stdout,
    reason = "the purpose of not having printing to stdout is so that tracing is used properly \
              for the core libraries. These are tests, so it doesn't matter"
)]

use flecs_ecs::core::{EntityViewGet, World, WorldGet};
use hyperion::simulation::{Position, Uuid, Velocity, entity_kind::EntityKind};

#[test]
fn arrow() {
    let world = World::new();
    world.import::<hyperion::HyperionCore>();

    let arrow = world.entity().add_enum(EntityKind::Arrow);

    assert!(
        arrow.has::<Uuid>(),
        "All entities should automatically be given a UUID."
    );

    arrow.get::<&Uuid>(|uuid| {
        assert_ne!(uuid.0, uuid::Uuid::nil(), "The UUID should not be nil.");
    });

    arrow
        .set(Velocity::new(0.0, 1.0, 0.0))
        .set(Position::new(0.0, 20.0, 0.0));

    println!("arrow = {arrow:?}\n");
    
    world.progress();
    
    world.get::<&Position>(|pos| {
        println!("pos = {pos:?}");
    });
}
