use flecs_ecs::{
    core::{
        flecs::pipeline, EntityViewGet, IdOperations, Query, QueryBuilderImpl, QueryTuple,
        SystemAPI, TermBuilderImpl, World, WorldRef,
    },
    macros::system,
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::{
    component,
    component::{
        blocks::MinecraftWorld,
        command::{Command, ROOT_COMMAND},
        Comms, InGameName, Pose, Uuid,
    },
    net::{Compose, NetworkStreamRef},
    runtime::AsyncRuntime,
    system::player_join_world::player_join_world,
    util::player_skin::PlayerSkin,
    SystemRegistry,
};

pub struct SendableRef<'a>(pub WorldRef<'a>);

unsafe impl<'a> Send for SendableRef<'a> {}
unsafe impl<'a> Sync for SendableRef<'a> {}

struct SendableQuery<T>(Query<T>)
where
    T: QueryTuple;

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<T: QueryTuple + Send> Send for SendableQuery<T> {}
unsafe impl<T: QueryTuple> Sync for SendableQuery<T> {}

pub fn joins(world: &'static World, registry: &mut SystemRegistry) {
    let query = world.new_query::<(&Uuid, &InGameName, &Pose, &PlayerSkin)>();

    let query = SendableQuery(query);

    let stages = (0..world.get_stage_count())
        .map(|stage| world.stage(stage))
        .map(SendableRef)
        .collect::<Vec<_>>();

    let system_id = registry.register();

    let root_command = world.entity().set(Command::ROOT);

    ROOT_COMMAND.set(root_command.id()).unwrap();

    let hello_command = world
        .entity()
        .set(Command::literal("hello"))
        .child_of_id(root_command);

    world
        .entity()
        .set(Command::literal("world"))
        .child_of_id(hello_command);

    let root_command = root_command.id();

    system!(
        "joins",
        world,
        &AsyncRuntime($),
        &Comms($),
        &MinecraftWorld($),
        &Compose($),
    )
    .kind::<pipeline::OnUpdate>()
    .each(move |(tasks, comms, blocks, compose)| {
        let span = tracing::trace_span!("joins");
        let _enter = span.enter();

        let mut skins = Vec::new();

        while let Ok(Some((entity, skin))) = comms.skins_rx.try_recv() {
            skins.push((entity, skin.clone()));
        }

        // todo: par_iter but bugs...
        // for (entity, skin) in skins {
        skins.into_par_iter().for_each(|(entity, skin)| {
            // if we are not in rayon context that means we are in a single-threaded context and 0 will work
            let idx = rayon::current_thread_index().unwrap_or(0);

            let world = &stages[idx];
            let world = world.0;

            if !world.is_alive(entity) {
                return;
            }

            let entity = world.entity_from_id(entity);

            entity.add::<component::Play>();

            entity.get::<(&Uuid, &InGameName, &Pose, &NetworkStreamRef)>(
                |(uuid, name, pose, &stream_id)| {
                    let query = &query;
                    let query = &query.0;

                    player_join_world(
                        &entity,
                        tasks,
                        blocks,
                        compose,
                        uuid.0,
                        name,
                        stream_id,
                        pose,
                        &world,
                        &skin,
                        system_id,
                        root_command,
                        query,
                    );
                },
            );

            let entity = world.entity_from_id(entity);
            entity.set(skin);
        });
    });
}
